use std::{io, io::prelude::*, num};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};

#[derive(Debug)]
pub enum DccError {
    IntError(num::ParseIntError),
    IoError(io::Error),
}
#[derive(Debug)]
pub struct Dcc {
    ip: IpAddr,
    port: u16,
    size: u32,
    pub msg: String,
}

impl Dcc {
    pub fn from_msg(msg: &str,) -> Result<Dcc, DccError> {
        let msg_split: Vec<&str> = msg.split_whitespace().collect();
        let i = msg_split.len()-1;
        let ip_num: u32 = match msg_split[i-2].parse() {
            Ok(v) => v,
            Err(e) => return Err(DccError::IntError(e)),
        };
        let ip: IpAddr = IpAddr::V4(Ipv4Addr::from(ip_num));
        let port: u16 = match msg_split[i-1].parse(){
            Ok(v) => v,
            Err(e) => return Err(DccError::IntError(e)),
        };
        let size_string: String = msg_split[i].chars().take_while(|c| c.is_numeric()).collect();
        let size: u32 = match size_string.parse(){
            Ok(v) => v,
            Err(e) => return Err(DccError::IntError(e)),
        };
        let dcc = Dcc{
           ip,
           port,
           size,
           msg: msg.to_string(),
        };
        Ok(dcc)
    }
    pub fn get_file(&self) -> Result<Vec<u8>, DccError>{
        let socket_addr = SocketAddr::new(self.ip, self.port);
        let mut socket = match TcpStream::connect(socket_addr){
            Ok(v) => v,
            Err(e) => return Err(DccError::IoError(e)),
        };
        let mut file = vec![];
        loop {
            let mut buf = vec![];
            match socket.read_to_end(&mut buf){
                Ok(v) => v,
                Err(e) => return Err(DccError::IoError(e)),
            };
            file.append(&mut buf);
            let ack: u32 = file.len() as u32;
            loop{
                match socket.write(&ack.to_ne_bytes()){
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Ok(_) => break,
                    Err(e) => return Err(DccError::IoError(e)),
                };
            }
            if ack >= self.size{
                break;
            };
        }
        Ok(file)
    }
}