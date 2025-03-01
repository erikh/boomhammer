use anyhow::Result;
use clap::Parser;
use fancy_duration::AsFancyDuration;
use hickory_resolver::{name_server::GenericConnector, TokioAsyncResolver};
use hyper::client::conn::http1::SendRequest;
use hyper::Request;
use hyper_util::rt::TokioIo;
use std::net::*;
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{channel, unbounded_channel, Receiver, Sender, UnboundedSender};
use tokio::sync::Mutex;

#[derive(Parser, Debug)]
#[command(
    author = "Erik Hollensbe <erik+github@hollensbe.org",
    about = "boomhammer: small HTTP load tester"
)]
struct Args {
    #[arg(name = "CPU Count", short = 'c', long = "cpu")]
    cpus: Option<usize>,
    url: String,
}

#[derive(Debug, Default)]
struct Stats {
    successes: AtomicUsize,
    failures: AtomicUsize,
}

impl std::ops::AddAssign<bool> for Stats {
    fn add_assign(&mut self, rhs: bool) {
        if rhs {
            self.successes.fetch_add(1, Ordering::SeqCst);
        } else {
            self.failures.fetch_add(1, Ordering::SeqCst);
        }
    }
}

#[derive(Debug, Clone)]
struct RequestBuilder {
    drain: bool,
    incoming: Arc<Mutex<Receiver<SendRequest<http_body_util::Empty<hyper::body::Bytes>>>>>,
    used: Sender<SendRequest<http_body_util::Empty<hyper::body::Bytes>>>,
    addr: SocketAddr,
    uri: String,
}

async fn url_to_addr(url: String) -> Result<SocketAddr> {
    let uri: hyper::Uri = url.parse()?;
    let default_port: &str = match uri.scheme_str() {
        Some("https") => "443",
        _ => "80",
    };

    let port = match uri.port() {
        Some(p) => p.to_string(),
        None => default_port.to_string(),
    };

    let host = uri.host().unwrap_or("127.0.0.1");

    let resolver = TokioAsyncResolver::from_system_conf(GenericConnector::default())?;
    let addr = match resolver.lookup_ip(format!("{}.", host)).await {
        Ok(response) => response.iter().next().unwrap(),
        Err(_) => IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
    };

    let str_addr = format!("{}:{}", addr.to_string(), port);

    Ok(SocketAddr::from_str(&str_addr)?)
}

impl RequestBuilder {
    fn new(addr: SocketAddr, uri: String) -> Result<Self> {
        let (used, incoming) = channel(1000);

        Ok(Self {
            used,
            incoming: Arc::new(Mutex::new(incoming)),
            addr,
            uri,
            drain: false,
        })
    }

    fn drain(&self, drain: bool) -> Self {
        let mut this = self.clone();
        this.drain = drain;
        this
    }

    async fn seed_connections(&self, close: Arc<Mutex<Receiver<()>>>) -> Result<()> {
        loop {
            let conn = tokio::net::TcpStream::connect(self.addr).await?;
            let io: TokioIo<TcpStream> = TokioIo::new(conn);

            let (s, c) = hyper::client::conn::http1::Builder::new()
                .handshake(io)
                .await?;
            tokio::spawn(c);

            self.used.send(s).await?;

            if let Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) =
                close.lock().await.try_recv()
            {
                return Ok(());
            }
        }
    }

    async fn worker(
        &self,
        close: Arc<Mutex<Receiver<()>>>,
        status: UnboundedSender<bool>,
    ) -> Result<()> {
        let this = self.clone();

        tokio::spawn(async move {
            loop {
                let recv = this.incoming.lock().await.try_recv();
                match recv {
                    Ok(mut s) => {
                        if !s.is_ready() {
                            if s.is_closed() {
                                continue;
                            }

                            let this = this.clone();
                            tokio::spawn(async move {
                                let _ = this.used.try_send(s);
                            });
                            continue;
                        }

                        let mut req = Request::get(this.uri.clone());
                        req.headers_mut().map(|h| {
                            h.insert(
                                "Host",
                                hyper::header::HeaderValue::from_str(&this.addr.to_string())
                                    .expect("Could not add Host header to request"),
                            )
                        });

                        let this = this.clone();
                        let status = status.clone();

                        tokio::spawn(async move {
                            let result = match s
                                .send_request(
                                    req.body::<http_body_util::Empty<hyper::body::Bytes>>(
                                        http_body_util::Empty::default(),
                                    )
                                    .unwrap(),
                                )
                                .await
                            {
                                Ok(resp) => {
                                    if this.drain {
                                        let _ = resp.into_body();
                                    }
                                    true
                                }
                                Err(_) => false,
                            };

                            status.send(result).expect("Could not send stats");

                            let this = this.clone();
                            tokio::spawn(async move {
                                let _ = this.used.try_send(s);
                            });
                        });
                    }
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                        if let Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) =
                            close.lock().await.try_recv()
                        {
                            return;
                        }
                    }
                    Err(_) => return,
                }
            }
        });

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let (s, mut r) = unbounded_channel();
    let (signal, close) = channel::<()>(1);
    let close = Arc::new(Mutex::new(close));

    let start = std::time::Instant::now();
    let url = args.url.clone();
    let addr = url_to_addr(args.url).await?;
    println!("{}", addr);
    for _ in 0..args.cpus.unwrap_or(num_cpus::get()) {
        let rb = RequestBuilder::new(addr, url.to_string())?;
        let w_close = close.clone();
        let worker = rb.clone();
        let s = s.clone();
        tokio::spawn(async move { worker.drain(true).worker(w_close, s).await.unwrap() });
        let s_close = close.clone();
        tokio::spawn(async move {
            rb.seed_connections(s_close)
                .await
                .expect("Unable to connect")
        });
    }

    drop(s);

    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::new(10, 0)).await;
        drop(signal);
    });

    let mut overall = Stats::default();

    while let Some(stats) = r.recv().await {
        overall += stats;
    }

    let time = std::time::Instant::now().duration_since(start);

    eprintln!(
        "Elapsed: {} - {:?} {} r/s",
        time.fancy_duration(),
        overall,
        overall.successes.load(Ordering::SeqCst) / time.as_secs() as usize,
    );

    Ok(())
}
