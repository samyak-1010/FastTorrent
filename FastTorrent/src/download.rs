use std::{
    collections::{HashSet, LinkedList}, fs::File, io::{Write, stdout}, net::{Ipv4Addr, SocketAddrV4}, os::unix::fs::FileExt, sync::Arc, time::Duration
};
use crossterm::{QueueableCommand, cursor, terminal, ExecutableCommand};
use sha1_smol::Sha1;
use tokio::{
    io::{AsyncWriteExt, AsyncReadExt},
    net::TcpStream,
    sync::Mutex,
    time::{timeout, sleep, self}
};
use byteorder::{BigEndian, ReadBytesExt};
use crate::{
    torrent_parser::{Torrent, Piece}, 
    message::{HandshakeMsg, Message}, 
    helpers::{self, BLOCK_SIZE, CONN_LIMIT, on_whole_msg}
};

pub async fn download_file(torrent: Torrent, file_ref: Arc<Vec<(File, u64)>>) {    

    let mut handles = vec![];
    loop {
        if *(torrent.piece_left.lock().await) == 0 {
            break;
        }

        while (*(torrent.connections.lock().await)).len() as u32 >= CONN_LIMIT || torrent.peer_list.lock().await.is_empty() {
            sleep(Duration::from_millis(1000)).await;
        }

        while !torrent.peer_list.lock().await.is_empty() {
            let mut q = torrent.peer_list.lock().await;
            let peer = (*q).pop_front().unwrap();

            let freq_ref  = torrent.piece_freq.clone();
            let file_ref = file_ref.clone();
            let down_ref = torrent.downloaded.clone();
            let conn_ref = torrent.connections.clone();
            let hashes = torrent.piece_hashes.clone();
            let left = torrent.piece_left.clone();

            if (*(conn_ref.lock().await)).contains(&peer) {
                continue;
            }

            let h = tokio::spawn( async move{

                let stream = connect(peer, torrent.info_hash, torrent.peer_id).await;
                if let Some(stream) = stream {
                    {
                        let mut connections = conn_ref.lock().await;
                        (*connections).insert(peer);
                    }
                    handle_connection(stream, freq_ref, file_ref, down_ref, hashes, left).await;
                    {
                        let mut connections = conn_ref.lock().await;
                        (*connections).remove(&peer);
                    }
                }
                else {
                    return;
                }
            });

            handles.push(h);
        }
        
        sleep(time::Duration::from_secs(5)).await;
    }

    for handle in handles {
        handle.await.unwrap();
    }

}

async fn connect(peer: (u32,u16), info_hash: [u8; 20], peer_id: [u8; 20]) -> Option<TcpStream> {

    let socket = SocketAddrV4::new(Ipv4Addr::from(peer.0),peer.1);
    let stream = timeout(tokio::time::Duration::from_secs(2),TcpStream::connect(socket)).await.ok()?.ok()?;

    handshake(stream, info_hash, peer_id).await

}

async fn handshake(mut stream: TcpStream, info_hash: [u8; 20], peer_id: [u8;20]) -> Option<TcpStream> {

    // Get handshake msg
    let mut handshake_msg = HandshakeMsg::build_msg(info_hash, peer_id);

    // Write handshake message to stream
    stream.write(&mut handshake_msg).await.unwrap();

    // Read handshake response
    let mut buf = Vec::new();
    let bytes_read = timeout(tokio::time::Duration::from_secs(2),stream.read_buf(&mut buf)).await.ok()?.ok()?;
    
    // Check whether response handshake or not
    if bytes_read == 64 && String::from_utf8_lossy(&buf[..=19])[1..] == "BitTorrent protocol".to_string() {
        // Read waste 4 bytes
        let mut tmp = [0; 4];
        let _res = timeout(tokio::time::Duration::from_secs(2),stream.read_exact(&mut tmp)).await;
        // Handle Torrent further from here
        Some(stream)
    }
    else {
        None
    }

}

async fn handle_connection(mut stream: TcpStream, freq_ref: Arc<Mutex<Vec<Piece>>>, file: Arc<Vec<(File, u64)>>, down_ref: Arc<Mutex<u64>>, hashes: Arc<Vec<Vec<u8>>>, piece_left: Arc<Mutex<u16>>) {

    let mut bitfield = vec![false; (*(freq_ref.lock().await)).len()];
    let mut choke = true;
    let mut requested: LinkedList<u32> = LinkedList::new();
    let mut piece_req: Option<usize> = None;

    loop {
        

        // Read length
        let mut msg;
        if let Some(len) = get_length(&mut stream).await {
            msg = on_whole_msg(&mut stream, len).await.unwrap();
        }
        else { 
            if !requested.is_empty() {

                let mut freq = freq_ref.lock().await;
                for begin in requested {
                    (*freq)[piece_req.unwrap()].blocks[begin as usize].is_req = false;
                }

            }
            return; 
        }

        
        // Read id of message
        let mut id = None;
        if msg.len() >= 1 { id = Some(msg[0]); }

        match id {
            None => {
                // keep-alive
            },
            Some(0) => {
                // choke
                choke = true;
            },
            Some(1) => {
                // unchoke
                choke = false;
            },
            Some(2) => {

                // Interested
                stream.write_all(&Message::build_unchoke()).await.unwrap();

            },
            Some(3) => {
                // not-interested
            },
            Some(4) => {

                // have
                let piece_index = ReadBytesExt::read_u32::<BigEndian>(&mut msg.as_mut_slice()[1..].as_ref()).unwrap();
                if !bitfield[piece_index as usize] {
                    (*(freq_ref.lock().await))[piece_index as usize].ref_no += 1;
                    bitfield[piece_index as usize] = true;
                }

            },
            Some(5) => {

                //bitfield
                let mut freq_arr = freq_ref.lock().await;
                for i in 1..msg.len() {
                    for (j, val) in helpers::u8_to_bin(msg[i as usize]).iter().enumerate() {

                        if !val { continue; }
                        let ind = (i as usize -1)*8 + j;
                        if ind >= bitfield.len() {
                            break;
                        }
                        bitfield[ind] = *val;
                        (*freq_arr)[ind].ref_no += 1;

                    }
                }

            },
            Some(6) => {

                // request
                let req = Message::read_request(&msg);
                println!("{:?}",req);

            },
            Some(7) => {

                let mut donwloaded = down_ref.lock().await;
                *donwloaded += (msg.len() - 9) as u64;

                let begin = write_to_file(msg, file.clone(), freq_ref.clone()).await;
                
                for (i, el) in requested.iter().enumerate() {
                    if *el == begin {
                        let mut split = requested.split_off(i);
                        split.pop_front();
                        requested.append(&mut split);
                        break;
                    }
                }
                
                if requested.is_empty() {

                    let piece_length;
                    let offset;
                    {
                        let freq = freq_ref.lock().await;
                        piece_length = (*freq)[piece_req.unwrap()].length;
                        offset = (*freq)[piece_req.unwrap()].blocks[0].offset;
                    }
                    
                    if !verify_piece(piece_length, offset, file.clone(), &(*hashes)[piece_req.unwrap()]) {
                        let mut freq = freq_ref.lock().await;

                        for block in &mut (*freq)[piece_req.unwrap()].blocks {
                            block.is_req = false;
                        }
                    }
                    else {

                        let mut freq = freq_ref.lock().await;
                        (*freq)[piece_req.unwrap()].completed = true;
                        
                        let mut left = piece_left.lock().await;
                        *left -= 1;
                        
                    }

                }

            },
            Some(8) => {
                // cancel
            },
            Some(9) => {
                // port
            },
            _ => {
                return;
            }
        }

        if !choke && requested.is_empty() {

            (requested, piece_req) = make_request(freq_ref.lock().await, &mut stream, &bitfield).await;
            if piece_req == None {return;}

        }

    }

}

pub fn verify_piece(piece_length: u64, offset: u64, file: Arc<Vec<(File,u64)>>, hash: &Vec<u8>) -> bool {

    let mut buf = vec![0u8; piece_length as usize];

    // Reading file at different locations
    let mut ind: usize = 0;
    let mut length: u64 = 0;
    while length <= offset  {
        length += (*file)[ind].1;
        ind += 1;
    }
    ind -= 1;

    // Read file
    let mut read = 0;
    while read != piece_length as usize && ind < (*file).len() {
        let res = (*file)[ind].0.read_at(&mut buf[read..], offset);

        // Return false if error in reading
        match res {
            Ok(bytes) => { read += bytes; },
            Err(_) => {
                return false; 
            }
        }

        ind += 1;
    }

    if read != piece_length as usize {
        return false;
    }

    // Generate hash
    let mut hasher = Sha1::new();
    hasher.update(&buf);

    // Validate hash
    if (*hash) == hasher.digest().bytes() {
        return true;
    }
    else {
        return false;
    }

}

async fn get_length(stream: &mut TcpStream) -> Option<u32> {

    let mut buf  = [0; 4];
    timeout(tokio::time::Duration::from_secs(120),stream.read_exact(&mut buf)).await.ok()?.ok()?;

    Some(ReadBytesExt::read_u32::<BigEndian>(&mut buf.as_ref()).unwrap())
}

async fn make_request(mut freq_arr: tokio::sync::MutexGuard<'_, Vec<Piece>>, stream: &mut TcpStream, bitfield: &Vec<bool> ) -> (LinkedList<u32>, Option<usize>) {

    let mut to_req = None;
    let mut mn = u16::MAX;

    // Find piece with minimum nodes
    for (i, piece) in (*freq_arr).iter_mut().enumerate() {
        if bitfield[i] && piece.ref_no < mn && piece.completed == false {

            for block in piece.blocks.iter() {

                if block.is_req == false {

                    to_req = Some(i);
                    mn = piece.ref_no;
                    break;

                }
            }
            
        }

    }

    let mut req = LinkedList::new();
    
    if to_req != None {
        
        let ind = to_req.unwrap();

        for (j, block) in (*freq_arr)[ind].blocks.iter_mut().enumerate() {
            if block.is_req == false {
                block.is_req = true;
                let res = stream.write(&Message::build_request(ind as u32, (j as u32)*BLOCK_SIZE, block.length as u32)).await;

                if let Err(_) = res {
                    return (req, None);
                }

                req.push_back(j as u32);
            }
        }
    }

    (req, to_req)
}

async fn write_to_file(mut msg: Vec<u8>, file: Arc<Vec<(File, u64)>>, freq_ref: Arc<Mutex<Vec<Piece>>>) -> u32 {

    // piece
    let buf = &mut msg.as_mut_slice()[1..].as_ref();
    let index = ReadBytesExt::read_u32::<BigEndian>(buf).unwrap();
    let mut begin = ReadBytesExt::read_u32::<BigEndian>(buf).unwrap();
    begin /= BLOCK_SIZE;

    let offset = (*freq_ref.lock().await)[index as usize].blocks[begin as usize].offset;

    // Writing to file at different locations
    let mut ind: usize = 0;
    let mut length: u64 = 0;
    while length <= offset  {
        length += (*file)[ind].1;
        ind += 1;
    }
    ind -= 1;
    let available = (*file)[ind].1 - offset;
    if available < (msg.len()-9) as u64 {
        ((*file)[ind]).0.write_at(&msg[9..(9 + available) as usize], offset).unwrap();
        ((*file)[ind+1]).0.write_at(&msg[(9 + available) as usize ..], offset).unwrap();
    }
    else {
        ((*file)[ind]).0.write_at(&msg[9..], offset).unwrap();
    }
    begin
}

pub async fn download_print(downloaded: Arc<Mutex<u64>>, connections: Arc<Mutex<HashSet<(u32,u16)>>>, piece_left: Arc<Mutex<u16>>) {
    let mut stdout = stdout();

    stdout.execute(cursor::Hide).unwrap();

    let mut last = 0;

    loop {

        let now; 
        let connection: usize; 
        let left;
        {
            now = *(downloaded.lock().await);
            connection = (*(connections.lock().await)).len();
            left = *(piece_left.lock().await);
        }

        if left == 0 {
            break;
        }

        let tot = (now as f64) / (1048756 as f64);
        let speed = ((now - last) as f64) / ((1048756*3) as f64);
        
        stdout.write_all(format!("\rDownloaded: {:.2} MB\nSpeed: {:.2} MB/s\nConnections: {}/{}\nPieces Left: {}", tot, speed, connection, CONN_LIMIT, left).as_bytes()).unwrap();
        
        stdout.execute(cursor::MoveUp(3)).unwrap();
        stdout.queue(terminal::Clear(terminal::ClearType::FromCursorDown)).unwrap();
        last = now;
        sleep(time::Duration::from_secs(3)).await;
    }
    stdout.execute(cursor::Show).unwrap();

    println!("Done!");
}