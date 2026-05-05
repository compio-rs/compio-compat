use std::{
    net::Ipv4Addr,
    time::{Duration, Instant},
};

use criterion::{Bencher, Criterion, Throughput, criterion_group, criterion_main};
use rand::{Rng, rng};

mod_use::mod_use![compat];

criterion_group!(net, echo);
criterion_main!(net);

const BUFFER_SIZE: usize = 524288;
const BUFFER_COUNT: usize = 8;

async fn echo_tokio_impl<T, R>(
    mut tx: T,
    mut rx: R,
    content: &[u8],
    client_buffer: &mut [u8],
    server_buffer: &mut [u8],
) where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    R: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let client = async move {
        for _i in 0..BUFFER_COUNT {
            tx.write_all(content).await.unwrap();
            tx.read_exact(client_buffer).await.unwrap();
        }
    };
    let server = async move {
        for _i in 0..BUFFER_COUNT {
            rx.read_exact(server_buffer).await.unwrap();
            rx.write_all(server_buffer).await.unwrap();
        }
    };
    tokio::join!(client, server);
}

async fn echo_compio_impl<T, R>(
    mut tx: T,
    mut rx: R,
    mut content: Vec<u8>,
    mut client_buffer: Vec<u8>,
    mut server_buffer: Vec<u8>,
) -> (Vec<u8>, Vec<u8>, Vec<u8>)
where
    T: compio::io::AsyncRead + compio::io::AsyncWrite,
    R: compio::io::AsyncRead + compio::io::AsyncWrite,
{
    use compio::io::{AsyncReadExt, AsyncWriteExt};

    let client = async move {
        for _i in 0..BUFFER_COUNT {
            (_, content) = tx.write_all(content).await.unwrap();
            (_, client_buffer) = tx.read_exact(client_buffer).await.unwrap();
        }
        (content, client_buffer)
    };
    let server = async move {
        for _i in 0..BUFFER_COUNT {
            (_, server_buffer) = rx.read_exact(server_buffer).await.unwrap();
            (_, server_buffer) = rx.write_all(server_buffer).await.unwrap();
        }
        server_buffer
    };
    let ((content, client_buffer), server_buffer) = futures_util::join!(client, server);
    (content, client_buffer, server_buffer)
}

fn echo_tokio_tcp(b: &mut Bencher, content: &[u8]) {
    use tokio::net::{TcpListener, TcpStream};

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    b.to_async(&runtime).iter_custom(|iter| async move {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();

        let mut client_buffer = vec![0u8; BUFFER_SIZE];
        let mut server_buffer = vec![0u8; BUFFER_SIZE];

        let start = Instant::now();
        for _i in 0..iter {
            let (tx, (rx, _)) =
                tokio::try_join!(TcpStream::connect(addr), listener.accept()).unwrap();
            echo_tokio_impl(tx, rx, content, &mut client_buffer, &mut server_buffer).await;
        }
        start.elapsed()
    })
}

async fn echo_compio_tcp_impl(iter: u64, mut content: Vec<u8>) -> Duration {
    use compio::net::{TcpListener, TcpStream};

    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();

    let mut client_buffer = vec![0u8; BUFFER_SIZE];
    let mut server_buffer = vec![0u8; BUFFER_SIZE];

    let start = Instant::now();
    for _i in 0..iter {
        let (tx, (rx, _)) =
            futures_util::try_join!(TcpStream::connect(addr), listener.accept()).unwrap();
        (content, client_buffer, server_buffer) =
            echo_compio_impl(tx, rx, content, client_buffer, server_buffer).await;
    }
    start.elapsed()
}

fn echo_compio_tcp(b: &mut Bencher, content: Vec<u8>) {
    let runtime = compio::runtime::Runtime::new().unwrap();
    b.to_async(&runtime)
        .iter_custom(|iter| echo_compio_tcp_impl(iter, content.clone()))
}

fn echo_tokio_compio_tcp(b: &mut Bencher, content: Vec<u8>) {
    let runtime = CompioInTokio::default();
    b.to_async(&runtime)
        .iter_custom(|iter| echo_compio_tcp_impl(iter, content.clone()))
}

#[cfg(unix)]
fn echo_tokio_unix(b: &mut Bencher, content: &[u8]) {
    use tokio::net::{UnixListener, UnixStream};

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    b.to_async(&runtime).iter_custom(|iter| async move {
        let dir = tempfile::Builder::new()
            .prefix("tokio-uds")
            .tempdir()
            .unwrap();
        let sock_path = dir.path().join("connect.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();

        let mut client_buffer = vec![0u8; BUFFER_SIZE];
        let mut server_buffer = vec![0u8; BUFFER_SIZE];

        let start = Instant::now();
        for _i in 0..iter {
            let (tx, (rx, _)) =
                tokio::try_join!(UnixStream::connect(&sock_path), listener.accept()).unwrap();
            echo_tokio_impl(tx, rx, content, &mut client_buffer, &mut server_buffer).await;
        }
        start.elapsed()
    })
}

async fn echo_compio_unix_impl(iter: u64, mut content: Vec<u8>) -> Duration {
    use compio::net::{UnixListener, UnixStream};

    let dir = tempfile::Builder::new()
        .prefix("compio-uds")
        .tempdir()
        .unwrap();
    let sock_path = dir.path().join("connect.sock");
    let listener = UnixListener::bind(&sock_path).await.unwrap();

    let mut client_buffer = vec![0u8; BUFFER_SIZE];
    let mut server_buffer = vec![0u8; BUFFER_SIZE];

    let start = Instant::now();
    for _i in 0..iter {
        let (tx, (rx, _)) =
            futures_util::try_join!(UnixStream::connect(&sock_path), listener.accept()).unwrap();
        (content, client_buffer, server_buffer) =
            echo_compio_impl(tx, rx, content, client_buffer, server_buffer).await;
    }
    start.elapsed()
}

fn echo_compio_unix(b: &mut Bencher, content: Vec<u8>) {
    let runtime = compio::runtime::Runtime::new().unwrap();
    b.to_async(&runtime)
        .iter_custom(|iter| echo_compio_unix_impl(iter, content.clone()))
}

fn echo_compio_in_tokio_unix(b: &mut Bencher, content: Vec<u8>) {
    let runtime = CompioInTokio::default();
    b.to_async(&runtime)
        .iter_custom(|iter| echo_compio_unix_impl(iter, content.clone()))
}

fn echo(c: &mut Criterion) {
    let mut rng = rng();

    let mut content = vec![0u8; BUFFER_SIZE];
    rng.fill_bytes(&mut content);

    let mut group = c.benchmark_group("echo");
    group.throughput(Throughput::Bytes((BUFFER_SIZE * BUFFER_COUNT * 2) as u64));

    group.bench_function("tokio-tcp", |b| echo_tokio_tcp(b, &content));
    group.bench_function("compio-tcp", |b| echo_compio_tcp(b, content.clone()));
    group.bench_function("compio-in-tokio-tcp", |b| {
        echo_tokio_compio_tcp(b, content.clone())
    });

    #[cfg(unix)]
    group.bench_function("tokio-unix", |b| echo_tokio_unix(b, &content));
    group.bench_function("compio-unix", |b| echo_compio_unix(b, content.clone()));
    group.bench_function("compio-in-tokio-unix", |b| {
        echo_compio_in_tokio_unix(b, content.clone())
    });

    group.finish();
}
