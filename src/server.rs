use crate::ipam::IpamPlugin;
use crate::types::*;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use std::convert::Infallible;
use std::sync::Arc;
use tokio::net::UnixListener;

/// HTTP server for the Docker IPAM plugin
pub struct PluginServer {
    plugin: Arc<IpamPlugin>,
}

impl PluginServer {
    pub fn new(plugin: Arc<IpamPlugin>) -> Self {
        Self { plugin }
    }

    /// Start the server on a Unix socket
    pub async fn serve_unix(self, socket_path: &str) -> anyhow::Result<()> {
        // Remove existing socket if it exists
        let _ = std::fs::remove_file(socket_path);

        // Ensure parent directory exists
        if let Some(parent) = std::path::Path::new(socket_path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(socket_path)?;
        tracing::info!("IPAM plugin listening on {}", socket_path);

        // Make socket accessible
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o666))?;
        }

        let plugin = self.plugin.clone();

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let plugin = plugin.clone();
                    tokio::spawn(async move {
                        let service = service_fn(move |req| {
                            let plugin = plugin.clone();
                            async move { handle_request(req, plugin).await }
                        });

                        if let Err(e) = hyper::server::conn::Http::new()
                            .serve_connection(stream, service)
                            .await
                        {
                            tracing::error!("Error serving connection: {}", e);
                        }
                    });
                }
                Err(e) => {
                    tracing::error!("Error accepting connection: {}", e);
                }
            }
        }
    }

    /// Start the server on a TCP port (for testing)
    pub async fn serve_tcp(self, addr: &str) -> anyhow::Result<()> {
        let addr = addr.parse()?;
        let plugin = self.plugin.clone();

        let make_svc = make_service_fn(move |_conn| {
            let plugin = plugin.clone();
            async move {
                Ok::<_, Infallible>(service_fn(move |req| {
                    let plugin = plugin.clone();
                    async move { handle_request(req, plugin).await }
                }))
            }
        });

        let server = Server::bind(&addr).serve(make_svc);
        tracing::info!("IPAM plugin listening on http://{}", addr);

        server.await?;
        Ok(())
    }
}

/// Handle incoming HTTP requests
async fn handle_request(
    req: Request<Body>,
    plugin: Arc<IpamPlugin>,
) -> Result<Response<Body>, Infallible> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    tracing::debug!("{} {}", method, path);

    let response = match (&method, path.as_str()) {
        (&Method::POST, "/Plugin.Activate") => json_response(serde_json::json!({
            "Implements": ["IpamDriver"]
        })),

        (&Method::POST, "/IpamDriver.GetCapabilities") => match plugin.get_capabilities().await {
            Ok(caps) => json_response(caps),
            Err(e) => error_response(&e.to_string()),
        },

        (&Method::POST, "/IpamDriver.GetDefaultAddressSpaces") => {
            json_response(serde_json::json!({
                "LocalDefaultAddressSpace": "local",
                "GlobalDefaultAddressSpace": "global"
            }))
        }

        (&Method::POST, "/IpamDriver.RequestPool") => {
            match parse_body::<RequestPoolRequest>(req).await {
                Ok(request) => match plugin.request_pool(request).await {
                    Ok(response) => json_response(response),
                    Err(e) => error_response(&e.to_string()),
                },
                Err(e) => error_response(&e),
            }
        }

        (&Method::POST, "/IpamDriver.ReleasePool") => {
            match parse_body::<ReleasePoolRequest>(req).await {
                Ok(request) => match plugin.release_pool(request).await {
                    Ok(_) => json_response(serde_json::json!({})),
                    Err(e) => error_response(&e.to_string()),
                },
                Err(e) => error_response(&e),
            }
        }

        (&Method::POST, "/IpamDriver.RequestAddress") => {
            match parse_body::<RequestAddressRequest>(req).await {
                Ok(request) => match plugin.request_address(request).await {
                    Ok(response) => json_response(response),
                    Err(e) => error_response(&e.to_string()),
                },
                Err(e) => error_response(&e),
            }
        }

        (&Method::POST, "/IpamDriver.ReleaseAddress") => {
            match parse_body::<ReleaseAddressRequest>(req).await {
                Ok(request) => match plugin.release_address(request).await {
                    Ok(_) => json_response(serde_json::json!({})),
                    Err(e) => error_response(&e.to_string()),
                },
                Err(e) => error_response(&e),
            }
        }

        _ => {
            tracing::warn!("Unknown endpoint: {} {}", method, path);
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("Not Found"))
                .unwrap()
        }
    };

    Ok(response)
}

/// Parse request body as JSON
async fn parse_body<T: serde::de::DeserializeOwned>(req: Request<Body>) -> Result<T, String> {
    let body_bytes = hyper::body::to_bytes(req.into_body())
        .await
        .map_err(|e| format!("Failed to read body: {}", e))?;

    serde_json::from_slice(&body_bytes).map_err(|e| format!("Failed to parse JSON: {}", e))
}

/// Create a JSON response
fn json_response<T: serde::Serialize>(data: T) -> Response<Body> {
    let json = serde_json::to_string(&data).unwrap();
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(json))
        .unwrap()
}

/// Create an error response
fn error_response(message: &str) -> Response<Body> {
    tracing::error!("Request failed: {}", message);
    json_response(ErrorResponse::new(message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Storage;
    use hyper::body::to_bytes;
    use std::sync::Arc;
    use tempfile::TempDir;

    async fn create_test_plugin() -> (Arc<IpamPlugin>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let state_file = temp_dir.path().join("state.yaml");
        let storage = Arc::new(Storage::new(&state_file).await.unwrap());
        let plugin = Arc::new(IpamPlugin::new(storage, "10.0.0.0/24".to_string()));
        (plugin, temp_dir)
    }

    #[tokio::test]
    async fn test_plugin_activate_endpoint() {
        let (plugin, _temp) = create_test_plugin().await;
        let req = Request::builder()
            .method(Method::POST)
            .uri("/Plugin.Activate")
            .body(Body::empty())
            .unwrap();

        let response = handle_request(req, plugin).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = to_bytes(response.into_body()).await.unwrap();
        let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
        assert!(body_str.contains("IpamDriver"));
    }

    #[tokio::test]
    async fn test_get_capabilities_endpoint() {
        let (plugin, _temp) = create_test_plugin().await;
        let req = Request::builder()
            .method(Method::POST)
            .uri("/IpamDriver.GetCapabilities")
            .body(Body::empty())
            .unwrap();

        let response = handle_request(req, plugin).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = to_bytes(response.into_body()).await.unwrap();
        let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
        assert!(body_str.contains("RequiresMACAddress"));
        assert!(body_str.contains("false"));
    }

    #[tokio::test]
    async fn test_get_default_address_spaces_endpoint() {
        let (plugin, _temp) = create_test_plugin().await;
        let req = Request::builder()
            .method(Method::POST)
            .uri("/IpamDriver.GetDefaultAddressSpaces")
            .body(Body::empty())
            .unwrap();

        let response = handle_request(req, plugin).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = to_bytes(response.into_body()).await.unwrap();
        let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
        assert!(body_str.contains("LocalDefaultAddressSpace"));
        assert!(body_str.contains("local"));
        assert!(body_str.contains("GlobalDefaultAddressSpace"));
        assert!(body_str.contains("global"));
    }

    #[tokio::test]
    async fn test_request_pool_endpoint() {
        let (plugin, _temp) = create_test_plugin().await;
        let body = serde_json::json!({
            "Pool": "192.168.1.0/24"
        });
        let req = Request::builder()
            .method(Method::POST)
            .uri("/IpamDriver.RequestPool")
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();

        let response = handle_request(req, plugin).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = to_bytes(response.into_body()).await.unwrap();
        let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
        assert!(body_str.contains("PoolID"));
        assert!(body_str.contains("192.168.1.0/24"));
    }

    #[tokio::test]
    async fn test_request_pool_invalid_subnet() {
        let (plugin, _temp) = create_test_plugin().await;
        let body = serde_json::json!({
            "Pool": "invalid-subnet"
        });
        let req = Request::builder()
            .method(Method::POST)
            .uri("/IpamDriver.RequestPool")
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();

        let response = handle_request(req, plugin).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = to_bytes(response.into_body()).await.unwrap();
        let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
        assert!(body_str.contains("Err") || body_str.contains("error"));
    }

    #[tokio::test]
    async fn test_request_address_endpoint() {
        let (plugin, _temp) = create_test_plugin().await;

        // First create a pool
        let pool_body = serde_json::json!({
            "Pool": "192.168.10.0/24"
        });
        let pool_req = Request::builder()
            .method(Method::POST)
            .uri("/IpamDriver.RequestPool")
            .header("Content-Type", "application/json")
            .body(Body::from(pool_body.to_string()))
            .unwrap();

        let pool_response = handle_request(pool_req, plugin.clone()).await.unwrap();
        let pool_body_bytes = to_bytes(pool_response.into_body()).await.unwrap();
        let pool_resp: RequestPoolResponse = serde_json::from_slice(&pool_body_bytes).unwrap();

        // Request an address
        let addr_body = serde_json::json!({
            "PoolID": pool_resp.pool_id,
            "Options": {
                "container_name": "test-container"
            }
        });
        let addr_req = Request::builder()
            .method(Method::POST)
            .uri("/IpamDriver.RequestAddress")
            .header("Content-Type", "application/json")
            .body(Body::from(addr_body.to_string()))
            .unwrap();

        let response = handle_request(addr_req, plugin).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = to_bytes(response.into_body()).await.unwrap();
        let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
        assert!(body_str.contains("Address"));
        assert!(body_str.contains("192.168.10."));
    }

    #[tokio::test]
    async fn test_release_address_endpoint() {
        let (plugin, _temp) = create_test_plugin().await;

        // Create pool and allocate address first
        let pool_body = serde_json::json!({"Pool": "192.168.20.0/24"});
        let pool_req = Request::builder()
            .method(Method::POST)
            .uri("/IpamDriver.RequestPool")
            .body(Body::from(pool_body.to_string()))
            .unwrap();
        let pool_response = handle_request(pool_req, plugin.clone()).await.unwrap();
        let pool_body_bytes = to_bytes(pool_response.into_body()).await.unwrap();
        let pool_resp: RequestPoolResponse = serde_json::from_slice(&pool_body_bytes).unwrap();

        let addr_body = serde_json::json!({
            "PoolID": pool_resp.pool_id,
        });
        let addr_req = Request::builder()
            .method(Method::POST)
            .uri("/IpamDriver.RequestAddress")
            .body(Body::from(addr_body.to_string()))
            .unwrap();
        let addr_response = handle_request(addr_req, plugin.clone()).await.unwrap();
        let addr_body_bytes = to_bytes(addr_response.into_body()).await.unwrap();
        let addr_resp: RequestAddressResponse = serde_json::from_slice(&addr_body_bytes).unwrap();

        // Release the address
        let release_body = serde_json::json!({
            "PoolID": pool_resp.pool_id,
            "Address": addr_resp.address
        });
        let release_req = Request::builder()
            .method(Method::POST)
            .uri("/IpamDriver.ReleaseAddress")
            .body(Body::from(release_body.to_string()))
            .unwrap();

        let response = handle_request(release_req, plugin).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_release_pool_endpoint() {
        let (plugin, _temp) = create_test_plugin().await;

        // Create a pool
        let pool_body = serde_json::json!({"Pool": "192.168.30.0/24"});
        let pool_req = Request::builder()
            .method(Method::POST)
            .uri("/IpamDriver.RequestPool")
            .body(Body::from(pool_body.to_string()))
            .unwrap();
        let pool_response = handle_request(pool_req, plugin.clone()).await.unwrap();
        let pool_body_bytes = to_bytes(pool_response.into_body()).await.unwrap();
        let pool_resp: RequestPoolResponse = serde_json::from_slice(&pool_body_bytes).unwrap();

        // Release the pool
        let release_body = serde_json::json!({
            "PoolID": pool_resp.pool_id
        });
        let release_req = Request::builder()
            .method(Method::POST)
            .uri("/IpamDriver.ReleasePool")
            .body(Body::from(release_body.to_string()))
            .unwrap();

        let response = handle_request(release_req, plugin).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_unknown_endpoint() {
        let (plugin, _temp) = create_test_plugin().await;
        let req = Request::builder()
            .method(Method::POST)
            .uri("/UnknownEndpoint")
            .body(Body::empty())
            .unwrap();

        let response = handle_request(req, plugin).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_invalid_json_body() {
        let (plugin, _temp) = create_test_plugin().await;
        let req = Request::builder()
            .method(Method::POST)
            .uri("/IpamDriver.RequestPool")
            .body(Body::from("invalid json"))
            .unwrap();

        let response = handle_request(req, plugin).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = to_bytes(response.into_body()).await.unwrap();
        let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
        assert!(body_str.contains("Err") || body_str.contains("error"));
    }

    #[tokio::test]
    async fn test_json_response_helper() {
        let data = serde_json::json!({
            "test": "value"
        });
        let response = json_response(data);
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "application/json"
        );

        let body_bytes = to_bytes(response.into_body()).await.unwrap();
        let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
        assert!(body_str.contains("test"));
        assert!(body_str.contains("value"));
    }

    #[tokio::test]
    async fn test_error_response_helper() {
        let response = error_response("Test error message");
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = to_bytes(response.into_body()).await.unwrap();
        let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
        assert!(body_str.contains("Err"));
        assert!(body_str.contains("Test error message"));
    }
}
