use tokio::{net::TcpStream, time::timeout, io::AsyncReadExt};

pub static BLOCK_SIZE: u32 = 16384; //2^14
pub static CONN_LIMIT: u32 = 100;
pub static QUEUE_LIMIT: u32 = 50;

// Convert u8 value to String of hex value
pub fn u8_to_hex(mut val: u8) -> String {

    let mut ans = String::new();
    for _ in 0..2 {
        ans.push(if val%16 <= 9 {
            ('0' as u8+ val%16) as char
        } else {
            ('A' as u8 + (val%16 - 10)) as char
        });
        val /= 16;
    }
    ans.chars().rev().collect()

}

// Convert byte array to url_encoded format
pub fn u8_to_url(arr: [u8; 20]) -> String {
    let mut ans = String::new();

    for byte in arr {
        if (byte >= ('0' as u8) && byte <= ('9' as u8)) 
        || (byte >= ('A' as u8) && byte <= ('Z' as u8)) 
        || (byte >= ('a' as u8) && byte <= ('z' as u8))
        || byte == '.' as u8 || byte == '-' as u8 || byte == '_' as u8 || byte == '~' as u8  {
            ans.push(byte as char);
        }
        else {
            ans.push('%');
            ans.push_str(&u8_to_hex(byte));
        }
    }

    ans
}

// Convert u8 value to binary
pub fn u8_to_bin(n: u8) -> Vec<bool> {
    let mut s = Vec::new();
    for i in (0..8).rev() {
        s.push(n&(1<<i) == 1<<i)
    }
    s
}

// Generate a random peer id
pub fn gen_random_id() -> [u8; 20] {

    let mut buf: [u8; 20] = [0;20];
    for i in  0..20 {
        buf[i] = rand::random();
    }

    buf

}

// Function which returns message from Tcp Stream only on getting message of entire length provided as input.
// Will terminate connection if expected length not recieved
pub async fn on_whole_msg(stream: &mut TcpStream, len: u32) -> Option<Vec<u8>> {

    let mut ret = Vec::new();
    while ret.len() < len as usize {
        let mut buf = [0];
        timeout(tokio::time::Duration::from_secs(20),stream.read_exact(&mut buf)).await.ok()?.ok()?;
        ret.push(buf[0]);
    }
    Some(ret)

}



#[cfg(test)]
mod tests {
    use crate::helpers::{u8_to_bin, u8_to_url};

    #[test]
    fn u8_to_bin_test() {
        assert_eq!(vec![true, true, true, true, true, true, true, true], u8_to_bin(255));
        assert_eq!(vec![false, false, false, false, true, false, false, false], u8_to_bin(8));
        assert_eq!(vec![false, false, false, false, true, false, true, true], u8_to_bin(11));
    }

    #[test]
    fn u8_to_url_test() {

        let arr: [u8; 20] = [ 18 , 52 , 86 , 120 , 154 , 188 , 222 , 241 , 35 , 69 , 103 , 137 , 171 , 205 , 239 , 18 , 52 , 86 , 120 , 154 ] ;
        assert_eq!(u8_to_url(arr), "%124Vx%9A%BC%DE%F1%23Eg%89%AB%CD%EF%124Vx%9A");

    }
}