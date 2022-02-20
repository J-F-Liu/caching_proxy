use ahash::AHashMap;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Request, Response, Server, Uri};
use lazy_static::lazy_static;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "InterceptProxy")]
struct Args {
    /// The IP:Port of the proxy server to listen on
    #[structopt(short, long, default_value = "127.0.0.1:8080")]
    pub listen: String,

    /// The rules to translate request urls
    #[structopt(short, long)]
    pub rules: String,
}

lazy_static! {
    static ref RULES: Arc<RwLock<AHashMap<String, String>>> =
        Arc::new(RwLock::new(AHashMap::new()));
}

fn parse_rules(rules: &str) -> Vec<(String, String)> {
    rules
        .split(";")
        .filter_map(|rule| {
            let items = rule.split("->").collect::<Vec<_>>();
            if items.len() == 2 {
                Some((items[0].to_string(), items[1].to_string()))
            } else {
                println!("Unrecognized rule: {rule}");
                None
            }
        })
        .collect::<Vec<_>>()
}

#[tokio::main]
pub async fn main() {
    let args = Args::from_args();

    // Initialize rules table and release the RwLock
    {
        let mut rules = RULES.write().unwrap();
        for (old, new) in parse_rules(&args.rules) {
            rules.insert(old, new);
        }
    }

    let addr = SocketAddr::from_str(&args.listen).unwrap();

    // For every connection, we must make a `Service` to handle all
    // incoming HTTP requests on said connection.
    let make_svc = make_service_fn(|_conn| {
        // This is the `Service` that will handle the connection.
        // `service_fn` is a helper to convert a function that
        // returns a Response into a `Service`.
        async { Ok::<_, Infallible>(service_fn(proxy)) }
    });

    let server = Server::bind(&addr)
        .serve(make_svc)
        .with_graceful_shutdown(shutdown_signal());

    println!("Listening on http://{}", addr);

    // Run this server for... forever!
    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}

async fn shutdown_signal() {
    // Wait for the CTRL+C signal
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C signal handler");
}

async fn proxy(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    let uri = req.uri();
    println!("{} {}", req.method(), uri.to_string());

    if uri.scheme_str() == Some("http") {
        let rules = RULES.clone();
        let host_port = if uri.port().is_none() {
            uri.host().unwrap_or_default().to_string()
        } else {
            format!("{}:{}", uri.host().unwrap_or_default(), uri.port().unwrap())
        };

        let new_req = if let Some(authority) = rules.read().unwrap().get(&host_port) {
            let mut uri_parts = uri.clone().into_parts();
            uri_parts.authority = http::uri::Authority::from_str(authority).ok();
            let new_uri = Uri::from_parts(uri_parts).unwrap();
            println!("=> {}", new_uri.to_string());

            let (mut parts, body) = req.into_parts();
            parts.uri = new_uri;
            Request::from_parts(parts, body)
        } else {
            req
        };

        let client = Client::new();
        client.request(new_req).await
    } else {
        let mut not_found = Response::default();
        *not_found.status_mut() = hyper::StatusCode::NOT_FOUND;
        Ok(not_found)
    }
}
