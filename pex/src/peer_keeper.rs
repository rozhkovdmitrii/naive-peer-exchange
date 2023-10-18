use derive_more::Display;
use futures::lock::{Mutex as AsyncMutex, MutexGuard};
use log::{debug, error, warn};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use wheel_timer::WheelTimer;

use crate::peer_keeper::PeerKeeperEvent::CheckValidity;
use crate::PeerInteractor;

const KEEPER_SCHEDULE_CAPACITY: usize = 100_000;
const VALIDATE_TIMEOUT_TICKS: usize = 3;
const KEEPER_TIME_WHEEL_TIC_SEC: u64 = 1;

pub(super) struct PeerKeeper {
    context: Arc<AsyncMutex<PeerKeeperContex>>,
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

struct PeerKeeperContex {
    connections: HashMap<u64, ConnectionInfo>,
    peers: HashMap<String, u64>,
    schedule: WheelTimer<PeerKeeperEvent>,
}

enum PeerKeeperEvent {
    /// Until the public address is not received through the connection it's considered as invalid
    /// that is why every connection should be checked for the validity after time and be thrown away if it's not
    CheckValidity { conn_id: u64 },
}

#[derive(Debug, Display)]
pub(super) enum PeerKeeperError {
    #[display(fmt = "Failed to accept peer, conn_id: {}", conn_id)]
    ConnIdExists { conn_id: u64 },
    #[display(fmt = "Connection not found, conn_id: {}", conn_id)]
    ConnectionNotFound { conn_id: u64 },
    #[display(
        fmt = "Public address: {} - is already owned by: {}, rejected for: {}",
        address,
        host_conn_id,
        conn_id
    )]
    ConflictingAddress {
        address: String,
        host_conn_id: u64,
        conn_id: u64,
    },
}

impl PeerKeeper {
    pub fn new() -> PeerKeeper {
        PeerKeeper {
            context: Arc::new(AsyncMutex::new(PeerKeeperContex {
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
                    CheckValidity { conn_id } => Self::remove_invalid_conn(&mut guard, conn_id),
                }
            }
        }
    }

    fn remove_invalid_conn(guard: &mut MutexGuard<'_, PeerKeeperContex>, conn_id: u64) {
        let Some(conn_info) = guard.connections.get(&conn_id) else {
            error!("Failed to get connection: {}", conn_id);
            return;
        };
        if conn_info.address.is_none() {
            warn!("Public address has not been assigned in time, deleted: {}", conn_id);
            guard.connections.remove(&conn_id);
        }
    }

    pub async fn on_new_connection(
        &self,
        conn: Arc<dyn PeerInteractor + Send>,
    ) -> Result<(), PeerKeeperError> {
        let mut guard = self.context.lock().await;
        let conn_id = conn.get_id();
        match guard.connections.try_insert(conn_id, ConnectionInfo::new(conn)) {
            Err(_) => Err(PeerKeeperError::ConnIdExists { conn_id }),
            Ok(new) => {
                debug!("Connection registered: {} - {}", conn_id, new.conn.get_address());
                guard.schedule.schedule(VALIDATE_TIMEOUT_TICKS, CheckValidity { conn_id });
                Ok(())
            }
        }
    }

    pub async fn set_peer_as_valid(
        &self,
        conn_id: u64,
        address: String,
    ) -> Result<(), PeerKeeperError> {
        let mut guard = self.context.lock().await;
        if !guard.connections.contains_key(&conn_id) {
            return Err(PeerKeeperError::ConnectionNotFound { conn_id });
        };
        if let Some(host_conn_id) = guard.peers.get(&address).cloned() {
            return Err(PeerKeeperError::ConflictingAddress {
                address,
                host_conn_id,
                conn_id,
            });
        }
        assert!(guard.peers.insert(address.clone(), conn_id).is_none());
        debug!("Public address: {} - assigned for the connection: {}", address, conn_id);
        Ok(())
    }

    pub(super) async fn on_peer_disconnected(&self, conn_id: u64) {
        self.drop_connection(conn_id).await;
    }

    pub(super) async fn on_peer_error(&self, conn_id: u64) {
        self.drop_connection(conn_id).await;
    }

    async fn drop_connection(&self, conn_id: u64) {
        let mut guard = self.context.lock().await;
        let Some(conn_info) = guard.connections.remove(&conn_id) else {
            warn!("Nothing removed from the registry by the conn_id: {}", conn_id);
            return;
        };
        if let Some(address) = conn_info.address {
            guard.peers.remove(&address);
        }
        debug!("Connection has been removed from registry: {}", conn_id);
    }

    pub(super) async fn get_peers(&self) -> Vec<String> {
        let guard = self.context.lock().await;
        guard.peers.keys().cloned().collect()
    }

    pub(super) async fn get_peers_and_conn_ids(&self) -> Vec<(u64, String)> {
        let guard = self.context.lock().await;
        guard.peers.iter().map(|(address, conn_id)| (*conn_id, address.clone())).collect()
    }

    pub(super) async fn get_valid_conns(&self) -> Vec<Arc<dyn PeerInteractor + Send>> {
        let guard = self.context.lock().await;
        let mut conns = vec![];
        for conn_id in guard.peers.values() {
            conns.push(
                guard
                    .connections
                    .get(conn_id)
                    .map(|conn_info| conn_info.conn.clone())
                    .expect("Expected connection to be in the registry")
                    .clone(),
            )
        }
        conns
    }

    pub(super) async fn is_address_known(&self, address: &str) -> bool {
        let context = self.context.lock().await;
        context.peers.contains_key(address)
    }

    pub(super) async fn get_connection(
        &self,
        conn_id: u64,
    ) -> Result<Arc<dyn PeerInteractor + Send>, PeerKeeperError> {
        let guard = self.context.lock().await;
        let conn_info = guard.connections.get(&conn_id);
        conn_info
            .map(|conn_info| conn_info.conn.clone())
            .ok_or_else(|| PeerKeeperError::ConnectionNotFound { conn_id })
    }
}
