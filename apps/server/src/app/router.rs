use super::*;

pub fn build_router(state: AppState) -> Router {
    let origins: Vec<HeaderValue> = state
        .settings
        .allowed_origins
        .iter()
        .filter_map(|origin| origin.parse().ok())
        .collect();
    let cors = CorsLayer::new()
        .allow_origin(origins)
        .allow_credentials(true)
        .allow_headers([
            http::header::CONTENT_TYPE,
            http::header::AUTHORIZATION,
            http::header::ACCEPT,
            HeaderName::from_static(PROTOCOL_VERSION_HEADER),
        ])
        .expose_headers([
            HeaderName::from_static(PROTOCOL_VERSION_HEADER),
            HeaderName::from_static(PROTOCOL_MIN_VERSION_HEADER),
            HeaderName::from_static(PROTOCOL_MAX_VERSION_HEADER),
            HeaderName::from_static(PROTOCOL_CAPABILITIES_HEADER),
        ])
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS]);

    let rate_limit_state = state.clone();
    let protocol_state = state.clone();
    Router::new()
        .nest("/api", api::router())
        .route("/ws", get(ws::websocket_handler))
        .layer(axum::extract::DefaultBodyLimit::max(64 * 1024))
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(15),
        ))
        .layer(CompressionLayer::new())
        .layer(CatchPanicLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(axum::middleware::from_fn_with_state(
            rate_limit_state,
            request_ip_rate_limit,
        ))
        .layer(axum::middleware::from_fn_with_state(
            protocol_state,
            protocol_compatibility,
        ))
        .layer(axum::middleware::from_fn(security_headers))
        .layer(cors)
        .with_state(state)
}

async fn protocol_compatibility(
    axum::extract::State(state): axum::extract::State<AppState>,
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    if !request.uri().path().starts_with("/api/") {
        return next.run(request).await;
    }
    let requested = match request.headers().get(PROTOCOL_VERSION_HEADER) {
        None => None,
        Some(value) => match value
            .to_str()
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
        {
            Some(version) => Some(version),
            None => {
                state
                    .metrics
                    .protocol_http_rejections
                    .fetch_add(1, Ordering::Relaxed);
                let mut response = GameError::ProtocolVersionMismatch.into_response();
                append_protocol_headers(&mut response, NegotiatedProtocol(crate::PROTOCOL_VERSION));
                return response;
            }
        },
    };
    let negotiated = match negotiate_protocol_version(requested) {
        Ok(negotiated) => {
            state.metrics.record_protocol_http_negotiation(negotiated.0);
            negotiated
        }
        Err(error) => {
            state
                .metrics
                .protocol_http_rejections
                .fetch_add(1, Ordering::Relaxed);
            let mut response = error.into_response();
            append_protocol_headers(&mut response, NegotiatedProtocol(crate::PROTOCOL_VERSION));
            return response;
        }
    };
    request.extensions_mut().insert(negotiated);
    let mut response = next.run(request).await;
    append_protocol_headers(&mut response, negotiated);
    response
}

fn append_protocol_headers(
    response: &mut axum::response::Response,
    negotiated: NegotiatedProtocol,
) {
    for (name, value) in [
        (PROTOCOL_VERSION_HEADER, negotiated.0.to_string()),
        (
            PROTOCOL_MIN_VERSION_HEADER,
            crate::MIN_SUPPORTED_PROTOCOL_VERSION.to_string(),
        ),
        (
            PROTOCOL_MAX_VERSION_HEADER,
            crate::MAX_SUPPORTED_PROTOCOL_VERSION.to_string(),
        ),
        (
            PROTOCOL_CAPABILITIES_HEADER,
            PROTOCOL_CAPABILITIES.join(","),
        ),
    ] {
        if let Ok(value) = HeaderValue::from_str(&value) {
            response
                .headers_mut()
                .insert(HeaderName::from_static(name), value);
        }
    }
}

async fn request_ip_rate_limit(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::ConnectInfo(address): axum::extract::ConnectInfo<SocketAddr>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, GameError> {
    let path = request.uri().path().to_string();
    let observe_availability = is_product_api_route(&path);
    let observe_command = is_product_http_command(&path);
    let started_at = std::time::Instant::now();
    state.metrics.http_requests.fetch_add(1, Ordering::Relaxed);
    if let Err(error) = state
        .enforce_ip_rate_limit(request.headers(), address)
        .await
    {
        if error == GameError::RateLimited {
            state
                .metrics
                .rate_limit_rejections
                .fetch_add(1, Ordering::Relaxed);
        }
        if observe_availability {
            state.metrics.record_http_response(error.status());
        }
        if observe_command {
            state.metrics.record_command_latency(
                CommandTransport::Http,
                false,
                started_at.elapsed(),
            );
        }
        return Err(error);
    }
    let response = next.run(request).await;
    if observe_availability {
        state.metrics.record_http_response(response.status());
    }
    if observe_command {
        state.metrics.record_command_latency(
            CommandTransport::Http,
            response.status().is_success() || response.status().is_redirection(),
            started_at.elapsed(),
        );
    }
    Ok(response)
}

pub(super) fn is_product_http_command(path: &str) -> bool {
    is_product_api_route(path) && !matches!(path, "/api/sessions" | "/api/accounts/login")
}

pub(super) fn is_product_api_route(path: &str) -> bool {
    path.starts_with("/api/")
        && !matches!(
            path,
            "/api/health"
                | "/api/ready"
                | "/api/metrics"
                | "/api/telemetry/funnel"
                | "/api/telemetry/performance"
        )
}

async fn security_headers(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        http::header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; base-uri 'self'; object-src 'none'; frame-ancestors 'none'; form-action 'self'; img-src 'self' data: blob:; font-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self'; connect-src 'self' ws: wss:; worker-src 'self' blob:",
        ),
    );
    headers.insert(
        http::header::STRICT_TRANSPORT_SECURITY,
        HeaderValue::from_static("max-age=31536000; includeSubDomains"),
    );
    headers.insert(
        http::header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        http::header::X_FRAME_OPTIONS,
        HeaderValue::from_static("DENY"),
    );
    headers.insert(
        http::header::REFERRER_POLICY,
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    headers.insert(
        http::header::HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=(), payment=()"),
    );
    headers.insert(
        http::header::HeaderName::from_static("cross-origin-opener-policy"),
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        http::header::HeaderName::from_static("cross-origin-resource-policy"),
        HeaderValue::from_static("same-origin"),
    );
    response
}
