mod app;
mod cli;
mod logging;

use clap::Parser;

use app::App;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    logging::init_logging();
    let args = cli::Cli::parse();
    let app = App::new(args.into());
    app.execute().await;
}
