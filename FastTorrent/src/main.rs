use std::{fs::{File, self, OpenOptions},env, sync::Arc, path::PathBuf};
use r_torrent::{
    torrent_parser::{Torrent, Piece},
    download,
    tracker::get_peers
};
use tokio::{sync::Mutex, time};

#[tokio::main]
async fn main() {
    
    // Open file and get decoded and info hash
    let mut args = env::args();
    if args.len() < 3 {
        panic!("usage: cargo run source_torrent destination_folder");
    }
    args.next();
    
    let dir = env::current_dir().unwrap();

    // Open .torrent file
    let source_dir = dir
        .join(args
                .next()
                .unwrap());
    let mut file = File::open(source_dir).unwrap();
    

    // All info mentioned in torrent file
    let mut torrent = Torrent::parse_decoded(&mut file).await.unwrap(); 
    
    // Initialize Destination file
    let destination_dir = dir
            .join(args
                .next()
                .unwrap())
            .join(&torrent.name.to_owned());
    
    // Create a file vector and pass it to download function
    let mut file_vec = Vec::new();
    
    // if multiple files
    if torrent.file_list != None {

        // Create dir based on destination dir
        fs::create_dir_all(&destination_dir).unwrap();

        // Create files inside that dir
        for (path, size) in torrent.file_list.unwrap() {
            let file_path = destination_dir.join(path);
            file_vec.push((open_file(file_path), size));
        }
        torrent.file_list = None;
    }
    else {
        file_vec.push(( open_file(destination_dir), torrent.length ));
    }


    // Distribute torrent info
    let (announce_url, announce_list) = (torrent.announce_url, torrent.announce_list);
    (torrent.announce_url, torrent.announce_list) = (None, None);
    
    let file_vec = Arc::new(file_vec);
    verify_file(torrent.piece_freq.clone(), file_vec.clone(), torrent.piece_hashes.clone(), torrent.downloaded.clone(), torrent.piece_left.clone()).await;
    
    // Get peers
    let h1 = get_peers(
        torrent.info_hash.clone(),
        torrent.length.clone(),
        torrent.peer_id.clone(),
        announce_url,
        torrent.peer_list.clone(),
        announce_list,
        torrent.connections.clone(),
        torrent.downloaded.clone(),
        torrent.piece_left.clone()
    );


    // Display function for downloading
    let h2 = download::download_print(torrent.downloaded.clone(), torrent.connections.clone(), torrent.piece_left.clone());


    // Download torrent
    let h3 = download::download_file(torrent, file_vec);


    tokio::join!(h1, h2, h3);

}

fn open_file(path: PathBuf) -> File {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)
        .unwrap() 
}

async fn verify_file(freq_ref: Arc<Mutex<Vec<Piece>>>, file_ref: Arc<Vec<(File,u64)>>, piece_hashes: Arc<Vec<Vec<u8>>>, downloaded: Arc<Mutex<u64>>, piece_left: Arc<Mutex<u16>>)  {

    println!("Checking already downloaded");

    let start = time::Instant::now();
    let len = (*freq_ref.lock().await).len();

    // let mut total: u64 = 0;
    let mut handles = Vec::new();
    
    for ind in 0..len {

        let freq = freq_ref.clone();
        let (downloaded, piece_left) = (downloaded.clone(), piece_left.clone());
        let (length, offset, file, hash);
        {
            let ref1 = freq.lock().await;
            (length, offset, file, hash) = ((*ref1)[ind].length, (*ref1)[ind].blocks[0].offset, file_ref.clone(), (*piece_hashes)[ind].clone())
        }

        let h = tokio::spawn(async move {

            if download::verify_piece(length, offset, file, &hash) {

                let mut ref1 = freq.lock().await;
                (*ref1)[ind].completed = true;

                let mut download = downloaded.lock().await;
                *download += (*ref1)[ind].length;

                let mut left = piece_left.lock().await;
                *left -= 1;
            }
        });
        handles.push(h);
    }

    for handle in handles {
        handle.await.unwrap();
    }
    
    let elapsed = start.elapsed();

    println!("Elapsed:{:.2?}\n",elapsed);

}