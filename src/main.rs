extern crate custom_error;

mod net;

use clap::Clap;
use net::{RequestListener, TcpRequestListener};

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opts: Opts = Opts::parse();

    println!("Binding to {}:{}", &opts.bind_address, opts.port);
    let mut listener = TcpRequestListener::new(&opts.bind_address, opts.port);
    listener.open()?;

    loop {
        if let Ok(mut stream) = listener.accept_request() {
            let mut result: String = String::new();
            stream.read_to_string(&mut result)?;
            println!("{:?}", &mut result);
        }
    }

    Ok(())
}
