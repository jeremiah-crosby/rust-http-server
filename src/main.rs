#![allow(dead_code)]

extern crate custom_error;

mod net;

use clap::Clap;
use flexi_logger::Logger;
use log::{debug, info};
use net::TcpRequestListener;
use std::{fs::read_to_string, path::Path};
use tokio::io::AsyncWriteExt;

use rust_http_parse::{parse_from_reader, HttpMethod, HttpRequest, ParseError};

/// This doc string acts as a help message when the user runs '--help'
/// as do all doc strings on fields
#[derive(Clap)]
#[clap(version = "1.0", author = "Jeremiah C. <jeremiahcrosby@gmail.com>")]
struct Opts {
    /// IP address to bind to
    #[clap(short, long, default_value = "127.0.0.1")]
    bind_address: String,
    /// A level of verbosity, and can be used multiple times
    #[clap(short, long, default_value = "80")]
    port: u32,
}

#[allow(unreachable_code)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    Logger::with_env_or_str("debug").start()?;

    let opts: Opts = Opts::parse();

    info!("Binding to {}:{}", &opts.bind_address, opts.port);
    let mut listener = TcpRequestListener::new(&opts.bind_address, opts.port);
    listener.open().await?;

    loop {
        if let Ok(mut stream) = listener.accept_request().await {
            let (mut read_half, mut write_half) = stream.split();
            let response = match parse_from_reader(&mut read_half).await {
                Ok(request) => {
                    debug!("Got request {:?}", &request);
                    handle_request(&request)
                }
                Err(ParseError::MaxHeaderSizeExceeded) => {
                    "HTTP/1.1 413 Entity Too Large\r\n\r\n".to_owned()
                }
                _ => "HTTP/1.1 500 Internal Server Error\r\n\r\n".to_owned(),
            };

            debug!("Sending response {}", &response);
            write_half.write(&response.as_bytes()).await?;
        }
    }

    Ok(())
}

fn handle_request(request: &HttpRequest) -> String {
    if request.method == HttpMethod::GET && request.path.starts_with("/static") {
        debug!("Handling static request");
        let stripped_path = Path::new(&request.path).strip_prefix("/static").unwrap();
        let final_path = Path::new("./files").join(&stripped_path);
        let content = read_to_string(final_path).unwrap();
        return format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
            content.len(),
            content
        );
    }

    "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n".to_string()
}
