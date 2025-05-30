mod storage;
mod file_storage;
mod http;
mod utils;

use std::sync::Arc;
use file_storage::FileStorage;

#[tokio::main]
pub async fn main() {
    let storage = FileStorage::new("test_storage");
    let storage = Arc::new(storage);
    let http_server = http::start_server(storage, 3000);
    http_server.await;
}

