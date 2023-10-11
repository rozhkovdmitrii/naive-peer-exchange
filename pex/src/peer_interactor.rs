use async_trait::async_trait;

pub(super) enum PeerError {}

pub(super) enum _PeerEvent {
    Message(String),
    Disconnected,
    ListOfPeers(Vec<String>),
}

#[async_trait]
pub(super) trait PeerInteractor {
    async fn send_random_message(&self) -> Result<(), PeerError>;
}

pub(super) struct PeerInteractorImpl {}

#[async_trait]
impl PeerInteractor for PeerInteractorImpl {
    async fn send_random_message(&self) -> Result<(), PeerError> {
        Ok(())
    }
}
