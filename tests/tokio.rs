use compio::{fs::File, io::AsyncReadAtExt, runtime::Runtime};
use compio_compat::{RuntimeCompat, TokioAdapter};
use tokio::io::AsyncReadExt;

#[tokio::test]
async fn fs() {
    tracing_subscriber::fmt()
        .with_max_level(compio_log::Level::TRACE)
        .init();

    let runtime = Runtime::new().unwrap();
    let runtime = RuntimeCompat::<TokioAdapter>::new(runtime).unwrap();
    let buffer = runtime
        .execute(async {
            let mut file = File::open("Cargo.toml").await.unwrap();
            let (_, buffer) = file.read_to_string_at(String::new(), 0).await.unwrap();
            buffer
        })
        .await;

    let mut file = tokio::fs::File::open("Cargo.toml").await.unwrap();
    let mut expected = String::new();
    file.read_to_string(&mut expected).await.unwrap();

    assert_eq!(buffer, expected);
}
