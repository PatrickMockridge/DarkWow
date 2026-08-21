/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * DarkWow is a tool for people and nations to establish sovereignty
 * according to human rights law. See the UN Declaration on the Rights
 * of Indigenous Peoples and associated documents:
 * https://documents.un.org/doc/undoc/gen/g26/031/70/pdf/g2603170.pdf
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{
    fmt::{self, Debug, Formatter},
    io::{self, ErrorKind},
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use arti_client::{
    config::{onion_service::OnionServiceConfigBuilder, BoolOrAuto, TorClientConfigBuilder},
    DataStream, StreamPrefs, TorClient,
};
use async_trait::async_trait;
use futures::{
    future::{select, Either},
    pin_mut,
    stream::StreamExt,
    Stream,
};
use smol::{
    lock::{Mutex as AsyncMutex, OnceCell},
    Timer,
};
use tor_cell::relaycell::msg::Connected;
use tor_error::ErrorReport;
use tor_hsservice::{HsNickname, RendRequest, RunningOnionService};
use tor_proto::client::stream::IncomingStreamRequest;
use tor_rtcompat::PreferredRuntime;
use tracing::{debug, error, warn};
use url::Url;

use super::{PtListener, PtStream};
use crate::util::{encoding::base32, logger::verbose, path::expand_path};

/// A static for `TorClient` reusability
static TOR_CLIENT: OnceCell<TorClient<PreferredRuntime>> = OnceCell::new();

/// Tor Dialer implementation
#[derive(Clone)]
pub struct TorDialer {
    client: TorClient<PreferredRuntime>,
}

impl Debug for TorDialer {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "TorDialer {{ TorClient }}")
    }
}

impl TorDialer {
    /// Instantiate a new [`TorDialer`] object
    pub(crate) async fn new(datastore: Option<String>) -> io::Result<Self> {
        // Initialize or fetch the static TOR_CLIENT that should be reused in
        // the Tor dialer
        let client = match TOR_CLIENT
            .get_or_try_init(|| async {
                debug!(target: "net::tor::TorDialer", "Bootstrapping...");
                if let Some(datadir) = &datastore {
                    #[expect(clippy::unwrap_used, reason = "expand_path always returns Ok")]
                    let datadir = expand_path(datadir).unwrap();
                    let arti_data = datadir.join("arti-data");
                    let arti_cache = datadir.join("arti-cache");

                    // HAZOP H19 fix: do NOT delete arti state on startup.
                    // Arti persists onion service keys internally.
                    #[expect(clippy::unwrap_used, reason = "config build from fixed directories cannot fail")]
                    let config = TorClientConfigBuilder::from_directories(arti_data, arti_cache)
                        .build()
                        .unwrap();

                    TorClient::create_bootstrapped(config).await
                } else {
                    TorClient::builder().create_bootstrapped().await
                }
            })
            .await
        {
            Ok(client) => client.isolated_client(),
            Err(e) => {
                warn!(target: "net::tor::TorDialer", "{}", e.report());
                return Err(io::Error::other("Internal Tor error, see logged warning"));
            }
        };

        Ok(Self { client })
    }

    /// Internal dial function
    pub(crate) async fn do_dial(
        &self,
        host: &str,
        port: u16,
        conn_timeout: Option<Duration>,
    ) -> io::Result<DataStream> {
        debug!(target: "net::tor::do_dial", "Dialing {host}:{port} with Tor...");

        let mut stream_prefs = StreamPrefs::new();
        stream_prefs.connect_to_onion_services(BoolOrAuto::Explicit(true));

        // If a timeout is configured, run both the connect and timeout futures
        // and return whatever finishes first. Otherwise, wait on the connect future.
        let connect = self.client.connect_with_prefs((host, port), &stream_prefs);

        match conn_timeout {
            Some(t) => {
                let timeout = Timer::after(t);
                pin_mut!(timeout);
                pin_mut!(connect);

                match select(connect, timeout).await {
                    Either::Left((Ok(stream), _)) => Ok(stream),

                    Either::Left((Err(e), _)) => {
                        warn!(target: "net::tor::do_dial", "{}", e.report());
                        Err(io::Error::other("Internal Tor error, see logged warning"))
                    }

                    Either::Right((_, _)) => Err(io::ErrorKind::TimedOut.into()),
                }
            }

            None => {
                match connect.await {
                    Ok(stream) => Ok(stream),
                    Err(e) => {
                        // Extract error reports (i.e. very detailed debugging)
                        // from arti-client in order to help debug Tor connections.
                        // https://docs.rs/arti-client/latest/arti_client/#reporting-arti-errors
                        // https://gitlab.torproject.org/tpo/core/arti/-/issues/1086
                        warn!(target: "net::tor::do_dial", "{}", e.report());
                        Err(io::Error::other("Internal Tor error, see logged warning"))
                    }
                }
            }
        }
    }
}

/// Tor Listener implementation
#[derive(Clone, Debug)]
pub struct TorListener {
    datastore: Option<String>,
    pub endpoint: Arc<OnceCell<Url>>,
}

impl TorListener {
    /// Instantiate a new [`TorListener`]
    pub async fn new(datastore: Option<String>) -> io::Result<Self> {
        Ok(Self { datastore, endpoint: Arc::new(OnceCell::new()) })
    }

    /// Internal listen function
    pub(crate) async fn do_listen(&self, port: u16) -> io::Result<TorListenerIntern> {
        // Initialize or fetch the static TOR_CLIENT that should be reused in
        // the Tor dialer
        let client = match TOR_CLIENT
            .get_or_try_init(|| async {
                debug!(target: "net::tor::do_listen", "Bootstrapping...");
                if let Some(datadir) = &self.datastore {
                    #[expect(clippy::unwrap_used, reason = "expand_path always returns Ok")]
                    let datadir = expand_path(datadir).unwrap();
                    let arti_data = datadir.join("arti-data");
                    let arti_cache = datadir.join("arti-cache");

                    // HAZOP H19 fix: do NOT delete arti state on startup.
                    // Arti persists onion service keys internally.
                    #[expect(clippy::unwrap_used, reason = "config build from fixed directories cannot fail")]
                    let config = TorClientConfigBuilder::from_directories(arti_data, arti_cache)
                        .build()
                        .unwrap();

                    TorClient::create_bootstrapped(config).await
                } else {
                    TorClient::builder().create_bootstrapped().await
                }
            })
            .await
        {
            Ok(client) => client.isolated_client(),
            Err(e) => {
                warn!(target: "net::tor::do_listen", "{}", e.report());
                return Err(io::Error::other("Internal Tor error, see logged warning"));
            }
        };

        #[expect(clippy::unwrap_used, reason = "dwow_tor is a valid onion service nickname")]
        let hs_nick = HsNickname::new("dwow_tor".to_string()).unwrap();

        let hs_config = match OnionServiceConfigBuilder::default().nickname(hs_nick).build() {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "net::tor::do_listen",
                    "[P2P] Failed to create OnionServiceConfig: {e}"
                );
                return Err(io::Error::other("Internal Tor error"));
            }
        };

        let (onion_service, rendreq_stream) = match client.launch_onion_service(hs_config) {
            Ok(Some(v)) => v,
            Ok(None) => {
                error!(
                    target: "net::tor::do_listen",
                    "[P2P] Onion service disabled in config",
                );
                return Err(io::Error::other("Internal Tor error"));
            }
            Err(e) => {
                error!(
                    target: "net::tor::do_listen",
                    "[P2P] Failed to launch Onion Service: {e}"
                );
                return Err(io::Error::other("Internal Tor error"));
            }
        };

        #[expect(clippy::unwrap_used, reason = "onion service launched above provides an address")]
        let onion_id =
            base32::encode(false, onion_service.onion_address().unwrap().as_ref()).to_lowercase();

        verbose!(
            target: "net::tor::do_listen",
            "[P2P] Established Tor listener on tor://{}:{port}", onion_id,
        );

        #[expect(clippy::unwrap_used, reason = "onion id and port form a valid tor URL")]
        let endpoint = Url::parse(&format!("tor://{onion_id}:{port}")).unwrap();
        #[expect(clippy::expect_used, reason = "endpoint OnceCell is set exactly once per listener")]
        let _ = self.endpoint.set(endpoint).await.expect("fatal endpoint already set for TorListener");

        Ok(TorListenerIntern {
            port,
            _onion_service: onion_service,
            rendreq_stream: AsyncMutex::new(Box::pin(rendreq_stream)),
        })
    }
}

/// Internal Tor Listener implementation, used with `PtListener`
pub struct TorListenerIntern {
    port: u16,
    _onion_service: Arc<RunningOnionService>,
    //rendreq_stream: Mutex<BoxStream<'a, RendRequest>>,
    rendreq_stream: AsyncMutex<Pin<Box<dyn Stream<Item = RendRequest> + Send>>>,
}

unsafe impl Sync for TorListenerIntern {}

#[async_trait]
impl PtListener for TorListenerIntern {
    async fn next(&self) -> io::Result<(Box<dyn PtStream>, Url)> {
        let mut rendreq_stream = self.rendreq_stream.lock().await;

        let Some(rendrequest) = rendreq_stream.next().await else {
            return Err(io::Error::new(ErrorKind::ConnectionAborted, "Connection Aborted"));
        };

        drop(rendreq_stream);

        let mut streamreq_stream = match rendrequest.accept().await {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "net::tor::PtListener::next",
                    "[P2P] Failed accepting Tor RendRequest: {e}"
                );
                return Err(io::Error::new(ErrorKind::ConnectionAborted, "Connection Aborted"));
            }
        };

        let Some(streamrequest) = streamreq_stream.next().await else {
            return Err(io::Error::new(ErrorKind::ConnectionAborted, "Connection Aborted"));
        };

        // Validate port correctness
        match streamrequest.request() {
            IncomingStreamRequest::Begin(begin) => {
                if begin.port() != self.port {
                    return Err(io::Error::new(ErrorKind::ConnectionAborted, "Connection Aborted"));
                }
            }
            &_ => return Err(io::Error::new(ErrorKind::ConnectionAborted, "Connection Aborted")),
        }

        let stream = match streamrequest.accept(Connected::new_empty()).await {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "net::tor::PtListener::next",
                    "[P2P] Failed accepting Tor StreamRequest: {e}"
                );
                return Err(io::Error::other("Internal Tor error"));
            }
        };

        #[expect(clippy::unwrap_used, reason = "static localhost URL always parses")]
        let url = Url::parse(&format!("tor://127.0.0.1:{}", self.port)).unwrap();
        Ok((Box::new(stream), url))
    }
}
