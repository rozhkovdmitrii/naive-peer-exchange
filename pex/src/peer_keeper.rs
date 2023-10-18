use derive_more::Display;
use futures::lock::{Mutex as AsyncMutex, MutexGuard};
use log::{debug, warn};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use wheel_timer::WheelTimer;

use crate::peer_keeper::PeerKeeperEvent::CheckValidity;
use crate::PeerInteractor;

const KEEPER_SCHEDULE_CAPACITY: usize = 100_000;
const VALIDATE_TIMEOUT_TICKS: usize = 1;
const KEEPER_TIME_WHEEL_TIC_SEC: u64 = 1;

pub(super) struct PeerKeeper {
    context: Arc<AsyncMutex<PeerKeeperContex>>,
}

struct PeerKeeperContex {
    connections: HashMap<u64, Arc<dyn PeerInteractor + Send>>,
    peers: HashMap<u64, String>,
    schedule: WheelTimer<PeerKeeperEvent>,
}

enum PeerKeeperEvent {
    /// Until the public address is not received through the connection it's considered as invalid
    /// that is why every connection should be checked for the validity after time and be thrown away if it's not
    CheckValidity { conn_id: u64 },
}

#[derive(Debug, Display)]
pub(super) enum PeerKeeperError {
    #[display(
        fmt = "Failed to accept peer, address already exists: {}, conn_id: {}",
        address,
        conn_id
    )]
    AddressExists { conn_id: u64, address: String },
    #[display(fmt = "Connection not found, conn_id: {}", conn_id)]
    ConnectionNotFound { conn_id: u64 },
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
        if !guard.peers.contains_key(&conn_id) {
            guard.connections.remove(&conn_id);
            warn!("Public address has not been received for the connection, deleted: {}", conn_id);
        }
    }

    pub async fn on_new_connection(
        &self,
        peer: Arc<dyn PeerInteractor + Send>,
    ) -> Result<(), PeerKeeperError> {
        let mut guard = self.context.lock().await;
        let conn_id = peer.get_id();
        match guard.connections.try_insert(conn_id, peer) {
            Err(old) => {
                let conn_id = old.value.get_id();
                let address = old.value.get_address();
                Err(PeerKeeperError::AddressExists {
                    conn_id,
                    address: address.to_string(),
                })
            }
            Ok(new) => {
                let conn_id = new.get_id();
                debug!("Connection registered: {} - {}", conn_id, new.get_address());
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

        if let Err(existent) = guard.peers.try_insert(conn_id, address.clone()) {
            return Err(PeerKeeperError::AddressExists {
                conn_id,
                address: existent.value,
            });
        }
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
        if guard.connections.remove(&conn_id).is_none() {
            debug!("Nothing removed from the connections by the conn_id: {}", conn_id);
        }
        if guard.peers.remove(&conn_id).is_some() {
            debug!("Connection has been removed from registry: {}", conn_id);
        };
    }

    pub(super) async fn get_peers(&self) -> Vec<String> {
        let guard = self.context.lock().await;
        guard.peers.values().cloned().collect()
    }

    pub(super) async fn get_peers_and_conn_ids(&self) -> Vec<(u64, String)> {
        let guard = self.context.lock().await;
        guard.peers.iter().map(|(conn_id, address)| (*conn_id, address.clone())).collect()
    }

    pub(super) async fn get_valid_conns(&self) -> Vec<Arc<dyn PeerInteractor + Send>> {
        let guard = self.context.lock().await;
        let mut conns = vec![];
        for conn_id in guard.peers.keys() {
            conns.push(guard.connections.get(conn_id).unwrap().clone())
        }
        conns
    }

    pub(super) async fn is_address_known(&self, address: &str) -> bool {
        let context = self.context.lock().await;

        for peer_address in context.peers.values() {
            if peer_address == address {
                return true;
            }
        }
        false
    }

    pub(super) async fn get_connection(
        &self,
        conn_id: u64,
    ) -> Result<Arc<dyn PeerInteractor + Send>, PeerKeeperError> {
        let guard = self.context.lock().await;
        let conn = guard.connections.get(&conn_id).cloned();
        conn.ok_or_else(|| PeerKeeperError::ConnectionNotFound { conn_id })
    }
}
