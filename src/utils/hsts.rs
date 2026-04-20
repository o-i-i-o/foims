use actix_web::http::header::{HeaderName, HeaderValue};
use actix_web::{
    Error,
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
};
use futures_util::future::LocalBoxFuture;
use std::future::Ready;

pub struct Hsts;

impl<S, B> Transform<S, ServiceRequest> for Hsts
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = HstsMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        std::future::ready(Ok(HstsMiddleware { service }))
    }
}

pub struct HstsMiddleware<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for HstsMiddleware<S>
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
        Box::pin(async move {
            let mut res = fut.await?;

            let hsts_value =
                HeaderValue::from_static("max-age=31536000; includeSubDomains; preload");

            res.headers_mut().insert(
                HeaderName::from_static("strict-transport-security"),
                hsts_value,
            );

            Ok(res)
        })
    }
}

#[must_use] 
pub const fn hsts_middleware() -> Hsts {
    Hsts
}
