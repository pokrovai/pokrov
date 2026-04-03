use pokrov_runtime::bootstrap::{parse_args, run};

#[tokio::main]
async fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match parse_args(&args) {
        Ok(parsed) => {
            if let Err(error) = run(parsed).await {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("{error}");
            eprintln!(
                "usage: cargo run -p pokrov-runtime -- --config <path>\n   or: cargo run -p pokrov-runtime -- --release-evidence-output <path> [--release-id <id>] [--artifact <path> ...]"
            );
            std::process::exit(2);
        }
    }
}
