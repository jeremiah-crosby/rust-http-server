#![allow(dead_code)]

extern crate custom_error;

mod http;
mod net;

use clap::Clap;
use flexi_logger::Logger;
use log::{debug, info};
use net::{RequestListener, TcpRequestListener};
use std::{fs::read_to_string, io::Write, net::Shutdown, path::Path};

use http::{parse_from_reader, HttpMethod, HttpRequest};

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
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Logger::with_env_or_str("debug").start()?;

    let opts: Opts = Opts::parse();

    info!("Binding to {}:{}", &opts.bind_address, opts.port);
    let mut listener = TcpRequestListener::new(&opts.bind_address, opts.port);
    listener.open()?;

    loop {
        if let Ok(mut stream) = listener.accept_request() {
            let request =
                parse_from_reader(Box::new(stream.try_clone().expect("Stream clone failed")))
                    .unwrap();
            debug!("Got request {:?}", &request);
            let response = handle_request(&request);
            debug!("Sending response {}", &response);
            stream.write(&response.as_bytes())?;
            stream.shutdown(Shutdown::Both).expect("Could not shutdown");
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
