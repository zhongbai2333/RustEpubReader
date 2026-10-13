//! Local validation client; only use an isolated test directory.
use reader_core::sharing::*;
use std::net::{SocketAddr, TcpStream};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    assert_eq!(
        args.len(),
        4,
        "usage: pin_probe ADDRESS PIN ISOLATED_TEST_DIRECTORY"
    );
    let directory = &args[3];
    let address: SocketAddr = args[1].parse().unwrap();
    std::fs::create_dir_all(directory).unwrap();
    let (private_key_pem, public_key_pem) = generate_rsa_keypair().unwrap();
    let mut store = PeerStore {
        device_id: uuid::Uuid::new_v4().to_string(),
        device_name: "Rust PIN Probe".into(),
        private_key_pem,
        public_key_pem,
        ..Default::default()
    };
    let outcome = (|| -> Result<(), String> {
        let stream = TcpStream::connect_timeout(&address, std::time::Duration::from_secs(10))
            .map_err(|e| e.to_string())?;
        let (mut stream, key, mut send, mut receive) =
            connect_with_stream(stream, &mut store, directory, None, Some(&args[2]))?;
        auto_sync_session(
            &mut stream,
            &key,
            &mut send,
            &mut receive,
            &mut store,
            directory,
            &format!("{directory}/books"),
            &[],
        )?;
        let books = reader_core::library::Library::load_from(directory).books;
        assert!(!books.is_empty());
        for book in &books {
            assert!(!reader_core::epub::EpubBook::open(&book.path)?
                .chapters
                .is_empty());
        }
        std::fs::write(std::path::Path::new(directory).join("verification.json"),serde_json::to_vec_pretty(&serde_json::json!({"paired":store.paired.len(),"books":books.len(),"valid_epubs":true})).unwrap()).unwrap();
        Ok(())
    })();
    match outcome {
        Ok(()) => {
            println!("PASS: PIN pairing, encrypted EPUB transfer and validated library import")
        }
        Err(_) => {
            println!("FAIL: PIN connection or transfer");
            std::process::exit(1);
        }
    }
}
