use actix_web::http::header::{HeaderName, HeaderValue};
use actix_web::{
    Error,
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
};
use futures_util::future::LocalBoxFuture;
use std::future::Ready;

pub struct AltSvc {
    port: Option<u16>,
}

impl AltSvc {
    pub fn new(enabled: bool, port: u16) -> Self {
        Self {
            port: if enabled { Some(port) } else { None },
        }
    }
}

impl<S, B> Transform<S, ServiceRequest> for AltSvc
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = AltSvcMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        std::future::ready(Ok(AltSvcMiddleware {
            service,
            port: self.port,
        }))
    }
}

pub struct AltSvcMiddleware<S> {
    service: S,
    port: Option<u16>,
}

impl<S, B> Service<ServiceRequest> for AltSvcMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    actix_web::dev::forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let fut = self.service.call(req);
        let port = self.port;
        Box::pin(async move {
            let mut res = fut.await?;

            if let Some(port) = port {
                let alt_svc_value = if port == 443 {
                    HeaderValue::from_static("h3=\":443\"; ma=86400")
                } else {
                    match HeaderValue::from_str(&format!("h3=\":{port}\"; ma=86400")) {
                        Ok(v) => v,
                        Err(_) => return Ok(res),
                    }
                };

                res.headers_mut()
                    .insert(HeaderName::from_static("alt-svc"), alt_svc_value);
            }

            Ok(res)
        })
    }
}
