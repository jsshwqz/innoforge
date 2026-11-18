//! 集成测试专用纯异步驱动 HTTP 辅助层（T-102 · 移除 `reqwest::blocking` 特性）
//!
//! 内部完全基于纯异步 `reqwest::Client` (`0.12`) + 独立单线程 Tokio Runtime 执行，
//! 对同步 `#[test]` 用例暴露无阻塞死锁风险的请求封装，彻底清退 `Cargo.toml` 中的 `blocking` 特性。

#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;

pub struct Client {
    rt: Arc<tokio::runtime::Runtime>,
    inner: reqwest::Client,
}

impl Clone for Client {
    fn clone(&self) -> Self {
        Self {
            rt: Arc::clone(&self.rt),
            inner: self.inner.clone(),
        }
    }
}

pub struct ClientBuilder {
    inner: reqwest::ClientBuilder,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    pub fn new() -> Self {
        Self::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("build sync_http test client")
    }

    pub fn builder() -> ClientBuilder {
        ClientBuilder {
            inner: reqwest::Client::builder().no_proxy(),
        }
    }

    pub fn get<U: reqwest::IntoUrl>(&self, url: U) -> RequestBuilder {
        let _guard = self.rt.enter();
        RequestBuilder {
            rt: Arc::clone(&self.rt),
            inner: self.inner.get(url),
        }
    }

    pub fn post<U: reqwest::IntoUrl>(&self, url: U) -> RequestBuilder {
        let _guard = self.rt.enter();
        RequestBuilder {
            rt: Arc::clone(&self.rt),
            inner: self.inner.post(url),
        }
    }

    pub fn put<U: reqwest::IntoUrl>(&self, url: U) -> RequestBuilder {
        let _guard = self.rt.enter();
        RequestBuilder {
            rt: Arc::clone(&self.rt),
            inner: self.inner.put(url),
        }
    }

    pub fn patch<U: reqwest::IntoUrl>(&self, url: U) -> RequestBuilder {
        let _guard = self.rt.enter();
        RequestBuilder {
            rt: Arc::clone(&self.rt),
            inner: self.inner.patch(url),
        }
    }

    pub fn delete<U: reqwest::IntoUrl>(&self, url: U) -> RequestBuilder {
        let _guard = self.rt.enter();
        RequestBuilder {
            rt: Arc::clone(&self.rt),
            inner: self.inner.delete(url),
        }
    }
}

impl ClientBuilder {
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.inner = self.inner.timeout(timeout);
        self
    }

    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.inner = self.inner.connect_timeout(timeout);
        self
    }

    pub fn redirect(mut self, policy: reqwest::redirect::Policy) -> Self {
        self.inner = self.inner.redirect(policy);
        self
    }

    pub fn no_proxy(mut self) -> Self {
        self.inner = self.inner.no_proxy();
        self
    }

    pub fn build(self) -> Result<Client, reqwest::Error> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build current_thread runtime for test client");
        let inner = {
            let _guard = rt.enter();
            self.inner.build()?
        };
        Ok(Client {
            rt: Arc::new(rt),
            inner,
        })
    }
}

pub struct RequestBuilder {
    rt: Arc<tokio::runtime::Runtime>,
    inner: reqwest::RequestBuilder,
}

impl RequestBuilder {
    pub fn header<K, V>(mut self, key: K, value: V) -> Self
    where
        reqwest::header::HeaderName: TryFrom<K>,
        <reqwest::header::HeaderName as TryFrom<K>>::Error: Into<axum::http::Error>,
        reqwest::header::HeaderValue: TryFrom<V>,
        <reqwest::header::HeaderValue as TryFrom<V>>::Error: Into<axum::http::Error>,
    {
        self.inner = self.inner.header(key, value);
        self
    }

    pub fn body<T: Into<reqwest::Body>>(mut self, body: T) -> Self {
        self.inner = self.inner.body(body);
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.inner = self.inner.timeout(timeout);
        self
    }

    pub fn bearer_auth<T: std::fmt::Display>(mut self, token: T) -> Self {
        self.inner = self.inner.bearer_auth(token);
        self
    }

    pub fn json<T: serde::Serialize + ?Sized>(mut self, json: &T) -> Self {
        self.inner = self.inner.json(json);
        self
    }

    pub fn multipart(mut self, form: multipart::Form) -> Self {
        self.inner = self.inner.multipart(form.inner);
        self
    }

    pub fn send(self) -> Result<Response, reqwest::Error> {
        let rt = Arc::clone(&self.rt);
        let inner = self.inner;
        let resp = rt.block_on(async move { inner.send().await })?;
        Ok(Response { rt, inner: resp })
    }
}

pub struct Response {
    rt: Arc<tokio::runtime::Runtime>,
    inner: reqwest::Response,
}

impl Response {
    pub fn status(&self) -> reqwest::StatusCode {
        self.inner.status()
    }

    pub fn headers(&self) -> &reqwest::header::HeaderMap {
        self.inner.headers()
    }

    pub fn content_length(&self) -> Option<u64> {
        self.inner.content_length()
    }

    pub fn text(self) -> Result<String, reqwest::Error> {
        let rt = self.rt;
        let inner = self.inner;
        rt.block_on(async move { inner.text().await })
    }

    pub fn bytes(self) -> Result<axum::body::Bytes, reqwest::Error> {
        let rt = self.rt;
        let inner = self.inner;
        rt.block_on(async move { inner.bytes().await })
    }

    pub fn json<T: serde::de::DeserializeOwned>(self) -> Result<T, reqwest::Error> {
        let rt = self.rt;
        let inner = self.inner;
        rt.block_on(async move { inner.json::<T>().await })
    }
}

pub fn get<U: reqwest::IntoUrl>(url: U) -> Result<Response, reqwest::Error> {
    Client::new().get(url).send()
}

pub mod multipart {
    pub struct Form {
        pub(super) inner: reqwest::multipart::Form,
    }

    impl Default for Form {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Form {
        pub fn new() -> Self {
            Self {
                inner: reqwest::multipart::Form::new(),
            }
        }

        pub fn text<T, U>(mut self, name: T, value: U) -> Self
        where
            T: Into<std::borrow::Cow<'static, str>>,
            U: Into<std::borrow::Cow<'static, str>>,
        {
            self.inner = self.inner.text(name, value);
            self
        }

        pub fn part<T>(mut self, name: T, part: Part) -> Self
        where
            T: Into<std::borrow::Cow<'static, str>>,
        {
            self.inner = self.inner.part(name, part.inner);
            self
        }
    }

    pub struct Part {
        pub(super) inner: reqwest::multipart::Part,
    }

    impl Part {
        pub fn bytes<T>(value: T) -> Self
        where
            T: Into<std::borrow::Cow<'static, [u8]>>,
        {
            Self {
                inner: reqwest::multipart::Part::bytes(value),
            }
        }

        pub fn file_name<T>(mut self, filename: T) -> Self
        where
            T: Into<std::borrow::Cow<'static, str>>,
        {
            self.inner = self.inner.file_name(filename);
            self
        }

        pub fn mime_str(self, mime: &str) -> Result<Self, reqwest::Error> {
            Ok(Self {
                inner: self.inner.mime_str(mime)?,
            })
        }
    }
}
