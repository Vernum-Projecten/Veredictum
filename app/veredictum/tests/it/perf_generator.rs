// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The open-loop generator against an in-process HTTP server on a real
//! socket, on real time: an instantly answering server at 2,000 arrivals per
//! second either leaves the generator's lag p99 under 5 ms or the window
//! reads `generator_bound`, so a generator that cannot keep up is named and
//! never published as the server's latency.

use std::sync::Arc;
use std::time::Duration;

use reqwest::StatusCode;
use veredictum::perf_gen::arrivals::{ArrivalProcess, Rate, RateProfile};
use veredictum::perf_gen::drive::{DrivePlan, GeneratorLimits, Stop, WindowSpec, drive};
use veredictum::perf_gen::{
    Call, ErrorClass, FailedExchange, Interaction, Mix, OperationId, Outcome, Performer,
};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Sends one `GET` per call over the async client.
struct HttpPerformer {
    client: reqwest::Client,
    url: String,
}

impl Performer for HttpPerformer {
    async fn perform(&self, _call: &Call) -> Outcome {
        match self.client.get(&self.url).send().await {
            Ok(response) if response.status() == StatusCode::OK => Outcome::Success,
            Ok(response) => {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                Outcome::Failure {
                    class: ErrorClass::for_status(status),
                    exchange: Some(FailedExchange::new(Some(status), "GET /", &body)),
                }
            }
            Err(e) if e.is_timeout() => Outcome::Failure {
                class: ErrorClass::Timeout,
                exchange: None,
            },
            Err(_) => Outcome::Failure {
                class: ErrorClass::Transport,
                exchange: None,
            },
        }
    }
}

#[test]
fn an_instant_server_at_2000_per_second_leaves_the_lag_under_5_ms_or_names_the_generator() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let limits = GeneratorLimits::default();
        let lag_limit = limits.lag_limit;
        let profile =
            RateProfile::constant(Rate::new(2_000.0).unwrap(), Duration::from_secs(4)).unwrap();
        let plan = DrivePlan::new(
            17,
            ArrivalProcess::Poisson,
            profile,
            vec![WindowSpec {
                start: Duration::from_secs(1),
                end: Duration::from_secs(4),
            }],
            limits,
        )
        .unwrap();
        let performer = Arc::new(HttpPerformer {
            client: reqwest::Client::builder()
                .pool_max_idle_per_host(4_096)
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap(),
            url: format!("{}/", server.uri()),
        });
        let mix = Mix::new(vec![
            Interaction::single("read", OperationId::new("read").unwrap(), 1).unwrap(),
        ])
        .unwrap();
        let report = drive(&plan, &mix, performer).await.unwrap();

        assert_eq!(report.stop, Stop::Completed);
        let window = &report.windows[0];
        let read = &window.operations[&OperationId::new("read").unwrap()];
        assert!(read.ok.len() > 5_000, "{} answers recorded", read.ok.len());
        assert!(read.errors.is_empty(), "{:?}", read.errors);
        assert_eq!(window.lag.len(), read.ok.len(), "every send has its lag");
        let lag_p99 = Duration::from_micros(window.lag.value_at_quantile(0.99));
        eprintln!("generator lag p99 at 2,000/s: {lag_p99:?}");
        assert!(
            lag_p99 < lag_limit || window.generator_bound(lag_limit),
            "lag p99 {lag_p99:?} over the limit and the window is not generator_bound"
        );
    });
}
