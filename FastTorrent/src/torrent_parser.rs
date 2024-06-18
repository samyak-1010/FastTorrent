use std::{
    collections::{VecDeque, HashSet}, sync::Arc, {fmt,fs::File}
};
use tokio::sync::Mutex;
use crate:: {
    bencoded_parser::{Bencode, Element},
    helpers::{self, BLOCK_SIZE}
};

pub struct Torrent {
    pub announce_url: Option<String>,
    pub announce_list: Option<Vec<String>>,
    pub name: String,
    pub length: u64,
    pub info_hash: [u8; 20],
    pub peer_list: Arc<Mutex<VecDeque<(u32,u16)>>>,
    pub peer_id: [u8; 20],
    pub piece_freq: Arc<Mutex<Vec<Piece>>>,
    pub downloaded: Arc<Mutex<u64>>,
    pub uploaded: Arc<Mutex<u64>>,
    pub connections: Arc<Mutex<HashSet<(u32,u16)>>>,
    pub file_list: Option<Vec<(String, u64)>>,
    pub piece_hashes: Arc<Vec<Vec<u8>>>,
    pub piece_left: Arc<Mutex<u16>>
}

#[derive(Clone)]
#[derive(Debug)]
pub struct Piece {
    pub ref_no: u16,
    pub length: u64,
    pub blocks: Vec<Block>,
    pub completed: bool
}

#[derive(Clone)]
#[derive(Debug)]
pub struct Block {
    pub is_req: bool,
    pub length: u64,
    pub offset: u64
}

impl Torrent {

    pub async fn parse_decoded(file: &mut File) -> Result<Torrent, InvalidTorrentFile> {

        let (decoded, info_hash) = Bencode::decode(file).unwrap();
        let (announce_url, announce_list, name, piece_length, hashes, length, piece_no, file_list) = Torrent::parse_decoded_helper(&decoded)?;

        let mut no_blocks = piece_length/(BLOCK_SIZE as u64);
        if piece_length/(BLOCK_SIZE as u64) != 0 { no_blocks += 1; }

        let torrent = Torrent { 
            announce_url, 
            announce_list, 
            name, 
            length, 
            info_hash, 
            peer_list: Arc::new(Mutex::new(VecDeque::new())), 
            peer_id: helpers::gen_random_id(), 
            piece_freq: Arc::new(Mutex::new(Torrent::build_piece_freq(no_blocks, piece_no, piece_length, length))),
            downloaded: Arc::new(Mutex::new(0)),
            uploaded: Arc::new(Mutex::new(0)),
            connections: Arc::new(Mutex::new(HashSet::new())),
            file_list,
            piece_hashes: Arc::new(hashes),
            piece_left: Arc::new(Mutex::new(piece_no as u16))
        };

        Ok(torrent)

    }

    // Function to return Announce Url, name, piece length and hashes from a decoded torrent file
    fn parse_decoded_helper(decoded: &Element) -> Result<(Option<String>, Option<Vec<String>>, String, u64, Vec<Vec<u8>>, u64, usize, Option<Vec<(String, u64)>>), InvalidTorrentFile> {

        let mut announce = None;
        let mut announce_list = None;
        let mut name = String::new();
        let mut piece_length = 0;
        let mut hashes = Vec::new();
        let mut length: u64 = 0;
        let mut files: Option<Vec<(String, u64)>> = None;
        let mut piece_no: usize = 0;

        match decoded {
            Element::Dict(mp) => {

                // Get List of announce urls
                if mp.contains_key("announce-list".as_bytes()) {
                    let mut tmp = Vec::new();
                    if let Element::List(l) = &mp["announce-list".as_bytes()] {
                        for i in l {
                            if let Element::List(l1) = i {
                                if let Element::ByteString(s) = &l1[0] {
                                    tmp.push(String::from_utf8(s.to_owned()).unwrap());
                                }
                            } 
                        }
                    }
                    announce_list = Some(tmp);
                }
                else { 
                    // Get Announce url of torrent file
                    if mp.contains_key("announce".as_bytes()) {
                        if let Element::ByteString(s) = &mp["announce".as_bytes()] { announce = Some(String::from_utf8(s.to_owned()).unwrap()); } 
                        else { return Err(InvalidTorrentFile{case: 0}); }
                    } 
                    else { return Err(InvalidTorrentFile{case: 1}); }
                }

                // Get info of torrent file
                if mp.contains_key("info".as_bytes()) {

                    match &mp["info".as_bytes()] {
                        Element::Dict(info_mp) => {
                            if !info_mp.contains_key("name".as_bytes()) || !info_mp.contains_key("piece length".as_bytes()) || !info_mp.contains_key("pieces".as_bytes()) || (!info_mp.contains_key("length".as_bytes()) && !info_mp.contains_key("files".as_bytes())) { return Err(InvalidTorrentFile{case: 6}); }

                            // Name
                            if let Element::ByteString(s) = &info_mp["name".as_bytes()] { name += &String::from_utf8_lossy(s); }
                            
                            // Piece Length
                            if let Element::Integer(l) = &info_mp["piece length".as_bytes()] { piece_length += l; }
                            
                            // Piece Hashes
                            if let Element::ByteString(s) = &info_mp["pieces".as_bytes()] {
                                piece_no = s.len()/20;

                                for i in (0..s.len()).step_by(20) {
                                    let tmp = s[i..(i+20)].to_vec();
                                    hashes.push(tmp);
                                }

                            }

                            // Length for single file
                            if info_mp.contains_key("length".as_bytes()) {
                                if let Element::Integer(l) = &info_mp["length".as_bytes()] { length += l.abs() as u64; }
                            }

                            // Length for multiple files
                            if info_mp.contains_key("files".as_bytes()) {

                                let mut file_list = Vec::new();

                                if let Element::List(files) = &info_mp["files".as_bytes()] {
                                    for file in files {
                                        if let Element::Dict(file_mp) = file {
                                            
                                            if let Element::Integer(l) = file_mp["length".as_bytes()] {

                                                length += l.abs() as u64;
                                                let mut path = String::new();

                                                if let Element::List(v) = &file_mp["path".as_bytes()] {
                                                    for s in v {
                                                        if let Element::ByteString(part) = s {
                                                            path += &(String::from_utf8(part.to_owned()).unwrap() + "/");
                                                        }
                                                    }
                                                }
                                                path.pop();
                                                file_list.push((path, l.abs() as u64));
                                            }


                                        }
                                    }
                                }
                                files = Some(file_list);
                            }

                        }
                        _ => { return Err(InvalidTorrentFile{case: 3}); }
                    }
                }
                else { return Err(InvalidTorrentFile { case: 4 }); }

            },
            _ => { return Err(InvalidTorrentFile{case: 5}); }
            
        }
        Ok((announce,announce_list,name,piece_length as u64, hashes, length, piece_no, files))
    }

    // Function to build the piece frequency array used by download
    fn build_piece_freq(no_blocks: u64, piece_no: usize, piece_length: u64, length: u64) -> Vec<Piece> {
        
        // Vector of all pieces, for each piece contains all blocks for each block a bool and the size of the block
        let mut piece_freq = vec! [
            Piece {
                ref_no: 0,
                length: piece_length,
                blocks: vec![
                        Block {
                            is_req: false,
                            length: BLOCK_SIZE as u64,
                            offset: 0
                        }; 
                        no_blocks as usize
                    ],
                completed: false
            };
            piece_no
        ];
        
        // Check whether pieces can be perfectly divided into blocks or last block of piece should be lesser in size
        if piece_length%(BLOCK_SIZE as u64) != 0 {
            for piece in &mut piece_freq {
                piece.blocks.last_mut().unwrap().length = piece_length%(BLOCK_SIZE as u64);
            }
        }
        
        // Check whether last piece has same number of blocks as other pieces
        let last_piece_length = length%piece_length;
        if last_piece_length != 0 {
            
            piece_freq.last_mut().unwrap().length = last_piece_length;

            let last_piece_block_no = last_piece_length/(BLOCK_SIZE as u64);

            while piece_freq.last().unwrap().blocks.len() != last_piece_block_no as usize {
                piece_freq.last_mut().unwrap().blocks.pop();
            }
            
            // Check whether last pieces last block is of BLOCK_SIZE or not
            if last_piece_length%(BLOCK_SIZE as u64) != 0 { 
                piece_freq.last_mut().unwrap().blocks.push(
                    Block {
                        is_req: false, 
                        length: (last_piece_length as u64)%(BLOCK_SIZE as u64), 
                        offset: 0
                    }
                );
            }
    
        }

        let mut curr: u64 = 0;

        for piece in &mut piece_freq {
            for block in &mut piece.blocks {
                (*block).offset = curr;
                curr += block.length;
            }
        }
    
        piece_freq
    }

}

#[derive(Debug)]
pub struct InvalidTorrentFile {
    case: i32
}

impl fmt::Display for InvalidTorrentFile {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Keys missing in torrent file {}", self.case)
    }
}