use derive_more::Display;
use futures::lock::{Mutex as AsyncMutex, MutexGuard};
use log::{debug, error, info, trace, warn};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use wheel_timer::WheelTimer;

use super::peer_interactor::PeerInteractor;

const KEEPER_SCHEDULE_CAPACITY: usize = 100_000;
const VALIDATE_TIMEOUT_TICKS: usize = 3;
const KEEPER_TIME_WHEEL_TIC_SEC: u64 = 1;

pub(super) struct PeerMng {
    context: Arc<AsyncMutex<PeerMngContex>>,
}

struct ConnectionInfo {
    conn: Arc<dyn PeerInteractor + Send>,
    address: Option<String>,
}

impl ConnectionInfo {
    fn new(conn: Arc<dyn PeerInteractor + Send>) -> ConnectionInfo {
        ConnectionInfo {
            conn,
            address: None,
        }
    }
}

struct PeerMngContex {
    connections: HashMap<u64, ConnectionInfo>,
    peers: HashMap<String, u64>,
    schedule: WheelTimer<PeerMngEvent>,
}

enum PeerMngEvent {
    /// Until the public address is not received through the connection it's considered as invalid
    /// that is why every connection should be checked for the validity after time and be thrown away if it's not
    CheckValidity { conn_id: u64 },
}

#[derive(Debug, Display)]
pub(super) enum PeerMngError {
    #[display(fmt = "Failed to accept peer, conn_id: {}", conn_id)]
    ConnIdExists { conn_id: u64 },
    #[display(fmt = "Connection not found, conn_id: {}", conn_id)]
    ConnectionNotFound { conn_id: u64 },
}

impl PeerMng {
    pub(super) fn new() -> PeerMng {
        PeerMng {
            context: Arc::new(AsyncMutex::new(PeerMngContex {
                connections: HashMap::default(),
                peers: HashMap::default(),
                schedule: WheelTimer::new(KEEPER_SCHEDULE_CAPACITY),
            })),
        }
    }

    pub(super) async fn execute(&self) {
        let context = self.context.clone();
        loop {
            tokio::time::sleep(Duration::from_secs(KEEPER_TIME_WHEEL_TIC_SEC)).await;
            let mut guard = context.lock().await;
            let events = guard.schedule.tick();
            for event in events {
                match event {
                    PeerMngEvent::CheckValidity { conn_id } => {
                        Self::remove_invalid_conn(&mut guard, conn_id)
                    }
                }
            }
        }
    }

    pub(super) async fn on_new_connection(
        &self,
        conn: Arc<dyn PeerInteractor + Send>,
    ) -> Result<(), PeerMngError> {
        let mut guard = self.context.lock().await;
        let conn_id = conn.get_id();
        match guard.connections.try_insert(conn_id, ConnectionInfo::new(conn)) {
            Err(_) => Err(PeerMngError::ConnIdExists { conn_id }),
            Ok(new) => {
                debug!("Connection registered: {} - {}", conn_id, new.conn.get_address());
                guard
                    .schedule
                    .schedule(VALIDATE_TIMEOUT_TICKS, PeerMngEvent::CheckValidity { conn_id });
                Ok(())
            }
        }
    }

    pub(super) async fn on_outgoing_peer_connected(
        &self,
        conn_id: u64,
        address: String,
    ) -> Result<(), PeerMngError> {
        let mut guard = self.context.lock().await;
        Self::set_peer_as_valid_impl(&mut guard, conn_id, address)
    }

    pub(super) async fn on_public_address(
        &self,
        conn_id: u64,
        address: String,
    ) -> Result<(Arc<dyn PeerInteractor + Send>, Vec<String>), PeerMngError> {
        // This address could be wrong and send by the malicious person that is why connection
        // should be proved by the negotiation procedure that is due to be implemented in the future
        let mut guard = self.context.lock().await;
        Self::set_peer_as_valid_impl(&mut guard, conn_id, address.clone())?;
        let conn = Self::get_connection_impl(&mut guard, conn_id)?;
        let mut known_peers = Self::get_peers_impl(&mut guard);
        known_peers.drain_filter(|peer_address| peer_address.as_str() == address.as_str());
        Ok((conn, known_peers))
    }

    pub(super) async fn on_peer_disconnected(&self, conn_id: u64) {
        self.drop_connection(conn_id).await;
    }

    pub(super) async fn on_peer_error(&self, conn_id: u64) {
        self.drop_connection(conn_id).await;
    }

    pub(super) async fn get_peers(&self) -> Vec<String> {
        let mut guard = self.context.lock().await;
        Self::get_peers_impl(&mut guard)
    }

    pub(super) async fn get_valid_conns(&self) -> Vec<Arc<dyn PeerInteractor + Send>> {
        let guard = self.context.lock().await;
        guard
            .connections
            .values()
            .filter(|conn_info| conn_info.address.is_some())
            .map(|conn_info| conn_info.conn.clone())
            .collect()
    }

    pub(super) async fn get_peer_address(&self, conn_id: u64) -> Option<String> {
        let context = self.context.lock().await;
        let conn_info = context.connections.get(&conn_id)?;
        conn_info.address.clone()
    }

    pub(super) async fn is_address_known(&self, address: &str) -> bool {
        let context = self.context.lock().await;
        trace!("peers: {:?}", context.peers);
        context.peers.contains_key(address)
    }

    fn set_peer_as_valid_impl(
        guard: &mut MutexGuard<'_, PeerMngContex>,
        conn_id: u64,
        address: String,
    ) -> Result<(), PeerMngError> {
        // It happens if there are cross connections that have not been validated yet
        if guard.peers.get(&address).is_some() {
            Self::drop_connection_impl(guard, conn_id);
            return Ok(());
        }
        let Some(mut conn_info) = guard.connections.get_mut(&conn_id) else {
            return Err(PeerMngError::ConnectionNotFound { conn_id });
        };
        conn_info.address = Some(address.clone());
        guard.peers.insert(address.clone(), conn_id);

        debug!("Public address: {} - assigned for the connection: {}", address, conn_id);
        info!("Connected to the peer at: {}", address);
        Ok(())
    }

    fn remove_invalid_conn(guard: &mut MutexGuard<'_, PeerMngContex>, conn_id: u64) {
        let Some(conn_info) = guard.connections.get(&conn_id) else {
            error!("Failed to get connection: {}", conn_id);
            return;
        };
        if conn_info.address.is_none() {
            warn!("Public address has not been assigned in time, deleted: {}", conn_id);
            guard.connections.remove(&conn_id);
        }
    }

    async fn drop_connection(&self, conn_id: u64) {
        let mut guard = self.context.lock().await;
        Self::drop_connection_impl(&mut guard, conn_id)
    }

    fn drop_connection_impl(guard: &mut MutexGuard<'_, PeerMngContex>, conn_id: u64) {
        let Some(conn_info) = guard.connections.remove(&conn_id) else {
            warn!("Nothing removed from the registry by the conn_id: {}", conn_id);
            return;
        };
        if let Some(address) = conn_info.address {
            guard.peers.remove(&address);
        }
        debug!("Connection has been removed from registry: {}", conn_id);
    }

    fn get_peers_impl(guard: &mut MutexGuard<'_, PeerMngContex>) -> Vec<String> {
        guard.peers.keys().cloned().collect()
    }

    fn get_connection_impl(
        guard: &mut MutexGuard<'_, PeerMngContex>,
        conn_id: u64,
    ) -> Result<Arc<dyn PeerInteractor + Send>, PeerMngError> {
        let conn_info = guard.connections.get(&conn_id);
        conn_info
            .map(|conn_info| conn_info.conn.clone())
            .ok_or(PeerMngError::ConnectionNotFound { conn_id })
    }
}
