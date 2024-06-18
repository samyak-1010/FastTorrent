use byteorder::{WriteBytesExt, BigEndian, ReadBytesExt};

#[allow(dead_code)]
#[derive(Debug)]
pub enum Message {
    KeepAlive {
        length: u32
    },
    Choke {
        length: u32,
        id: u8
    },
    Unchoke {
        length: u32,
        id: u8
    },
    Interested {
        length: u32,
        id: u8
    },
    Uninterested {
        length: u32,
        id: u8
    },
    Have {
        length: u32,
        id: u8,
        piece_index: u32
    },
    BitField {
        length: u32,
        id: u8,
        bitfield: Vec<u8>
    },
    Request {
        length: u32,
        id: u8,
        index: u32,
        begin: u32,
        req_length: u32
    },
    Piece {
        length: u32,
        id: u8,
        index: u32,
        begin: u32,
        block: Vec<u8>
    },
    Cancel {
        length: u32,
        id: u8,
        index: u32,
        begin: u32,
        req_length: u32
    },
    Port {
        length: u32,
        id: u8,
        listen_port: u16
    },
}

#[allow(dead_code)]
impl Message {
    fn build_keep_alive() -> Message {
        Message::KeepAlive { length: 0 }
    }

    fn build_choke() -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        buf.write_u32::<BigEndian>(1).unwrap();
        buf.write_u8(0).unwrap();
        buf
    }

    pub fn build_unchoke() -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        buf.write_u32::<BigEndian>(1).unwrap();
        buf.write_u8(1).unwrap();
        buf
    }

    fn build_interested() -> Message {
        Message::Interested { length: 1, id: 2 }
    }

    fn build_uninterested() -> Message {
        Message::Uninterested { length: 1, id: 3 }
    }

    fn build_have(piece_index:u32 ) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.write_u32::<BigEndian>(5).unwrap();
        buf.write_u8(4).unwrap();
        buf.write_u32::<BigEndian>(piece_index).unwrap();
        buf
    }

    fn build_bitfield(bitfield: Vec<u8>) -> Message {
        Message::BitField { length: 1+(bitfield.len() as u32), id: 5, bitfield}
    }

    pub fn build_request(index: u32, begin: u32, req_length: u32) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        buf.write_u32::<BigEndian>(13).unwrap();
        buf.write_u8(6).unwrap();
        buf.write_u32::<BigEndian>(index).unwrap();
        buf.write_u32::<BigEndian>(begin).unwrap();
        buf.write_u32::<BigEndian>(req_length).unwrap();
        buf
    }

    pub fn read_request(mut msg: &[u8]) -> Message {
        Message::Request { 
            length: 13, 
            id: msg.read_u8().unwrap(), 
            index: msg.read_u32::<BigEndian>().unwrap(),
            begin: msg.read_u32::<BigEndian>().unwrap(), 
            req_length: msg.read_u32::<BigEndian>().unwrap() 
        }
    }

    fn build_piece(index: u32, begin: u32, block: Vec<u8>) -> Message {
        Message::Piece { length: 9+(block.len() as u32), id: 7, index, begin, block }
    }

    fn build_cancel(index: u32, begin: u32, req_length: u32) -> Message {
        Message::Cancel { length: 13, id: 8, index, begin, req_length }
    }

    fn build_port(listen_port: u16) -> Message {
        Message::Port { length: 3, id: 9, listen_port }
    } 
}

pub struct HandshakeMsg {
    pstrlen: u8,
    pstr: String,
    reserved: u64,
    info_hash: [u8; 20],
    peer_id: [u8; 20]
}

impl HandshakeMsg {

    pub fn build_msg( info_hash: [u8; 20], peer_id: [u8;20]) -> Vec<u8> {

        let handshake = HandshakeMsg {
            pstrlen: 19,
            pstr: "BitTorrent protocol".to_string(),
            reserved: 0,
            info_hash,
            peer_id
        };

        let mut buf = Vec::new();

        buf.write_u8(handshake.pstrlen).unwrap(); // pstrlen
        //pstr
        for byte in handshake.pstr.as_bytes() {
            buf.write_u8(*byte).unwrap();
        }
        buf.write_u64::<BigEndian>(handshake.reserved).unwrap(); // reserved
        // info_hash
        for byte in handshake.info_hash {
            buf.write_u8(byte).unwrap();
        }
        // peer_id
        for byte in handshake.peer_id {
            buf.write_u8(byte).unwrap();
        }

        buf
    }
}

#[cfg(test)]
mod tests {
    use crate::helpers::gen_random_id;

    use super::HandshakeMsg;

    #[test]
    fn test_build_msg() {


        let buf = HandshakeMsg::build_msg(gen_random_id(), gen_random_id());
        assert_eq!(buf.len(), 68);

    }
}