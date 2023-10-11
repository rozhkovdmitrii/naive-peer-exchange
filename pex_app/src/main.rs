mod cli;
mod logging;
mod pex_app;

use clap::Parser;
use tokio;

use pex_app::PexApp;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    logging::init_logging();
    let pex_config_args = cli::Cli::parse();
    let app = PexApp::new(pex_config_args.into());
    app.execute().await;
}
