use futures::lock::{Mutex as AsyncMutex, MutexGuard};
use log::{debug, error, warn};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use wheel_timer::WheelTimer;

use crate::peer_keeper::PeerKeeperEvent::CheckValidity;
use crate::PeerInteractor;

const KEEPER_SCHEDULE_CAPACITY: usize = 100_000;
const VALIDATE_TIMEOUT_SEC: usize = 3;

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
    CheckValidity { invalidated_id: u64 },
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
            tokio::time::sleep(Duration::from_secs(1)).await;
            let mut guard = context.lock().await;
            let events = guard.schedule.tick();
            for event in events {
                match event {
                    CheckValidity {
                        invalidated_id: conn_id,
                    } => Self::check_address_valid(&mut guard, conn_id),
                }
            }
        }
    }

    fn check_address_valid(guard: &mut MutexGuard<'_, PeerKeeperContex>, conn_id: u64) {
        if !guard.peers.contains_key(&conn_id) {
            guard.connections.remove(&conn_id);
            warn!("Public address has not been received for the connection, deleted: {}", conn_id);
        }
    }

    pub async fn on_new_peer(&self, peer: Arc<dyn PeerInteractor + Send>) {
        let mut guard = self.context.lock().await;
        match guard.connections.try_insert(peer.get_id(), peer) {
            Err(old) => {
                let conn_id = old.value.get_id();
                let address = old.value.get_address();
                error!("Connection rejected: {}, connected from address: {}", conn_id, address);
            }
            Ok(new) => {
                let conn_id = new.get_id();
                debug!("Peer registered: {} - {}", conn_id, new.get_address());
                guard.schedule.schedule(
                    VALIDATE_TIMEOUT_SEC,
                    CheckValidity {
                        invalidated_id: conn_id,
                    },
                );
            }
        }
    }

    pub async fn on_peer_public_address(&self, conn_id: u64, address: String) {
        let mut guard = self.context.lock().await;

        if !guard.connections.contains_key(&conn_id) {
            error!("Failed to process on_peer_info, connection not found: {}", conn_id);
            return;
        };

        if let Err(existent) = guard.peers.try_insert(conn_id, address.clone()) {
            warn!(
                "Reassigning public address from: {}, to: {} blocked for: {}",
                existent, address, conn_id
            );
        }
        debug!("Public address: {} - assigned for the connection: {}", address, conn_id);
    }
}
