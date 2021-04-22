use custom_error::custom_error;
use log::debug;
use std::io::Read;
use tokio::net::{TcpListener, TcpStream};

custom_error! {pub NetError
    NotOpened = "TCP stream used before opened",
    IoError{source: std::io::Error} = "I/O Error: {}"
}

pub struct TcpRequestListener {
    address: String,
    port: u32,
    listener: Option<TcpListener>,
}

impl TcpRequestListener {
    pub fn new(address: &str, port: u32) -> Self {
        TcpRequestListener {
            address: address.to_owned(),
            port,
            listener: None,
        }
    }

    pub async fn open(&mut self) -> Result<(), NetError> {
        match TcpListener::bind(format!("{}:{}", self.address, self.port)).await {
            Ok(opened) => {
                self.listener = Some(opened);
                return Ok(());
            }
            Err(e) => Err(NetError::IoError { source: e }),
        }
    }

    pub async fn accept_request(&self) -> Result<TcpStream, NetError> {
        if let Some(ref listener) = self.listener {
            match listener.accept().await {
                Ok((stream, _)) => {
                    debug!("Connection established");
                    Ok(stream)
                }
                Err(e) => Err(NetError::IoError { source: e }),
            }
        } else {
            Err(NetError::NotOpened)
        }
    }
}
