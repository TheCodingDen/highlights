// Copyright 2020 Benjamin Scherer
// Licensed under the Open Software License version 3.0

//! Performance monitoring with Prometheus.

use hyper::{
	header::CONTENT_TYPE,
	service::{make_service_fn, service_fn},
	Body, Request, Response, Server,
};
use once_cell::sync::{Lazy, OnceCell};
use prometheus::{
	core::Collector, proto::MetricFamily, register_gauge_vec, Encoder,
	GaugeVec, TextEncoder,
};

use std::{
	net::{SocketAddr, ToSocketAddrs},
	time::Instant,
};

use crate::global::settings;

/// Indicator of whether performance monitoring is enabled or not.
static ENABLED: OnceCell<bool> = OnceCell::new();

/// Gauge of command execution time.
static COMMAND_TIME_GAUGE: Lazy<GaugeVec, fn() -> GaugeVec> = Lazy::new(|| {
	register_gauge_vec!(
		concat!(env!("CARGO_PKG_NAME"), "_command_time"),
		"Command execution time, in seconds",
		&["name"]
	)
	.unwrap()
});

/// Gauge of keyword notification execution time.
static NOTIFY_TIME_GAUGE: Lazy<GaugeVec, fn() -> GaugeVec> = Lazy::new(|| {
	register_gauge_vec!(
		concat!(env!("CARGO_PKG_NAME"), "_notify_time"),
		"Notification time time, in seconds",
		&["name"]
	)
	.unwrap()
});

/// Gauge of database query execution time.
static QUERY_TIME_GAUGE: Lazy<GaugeVec, fn() -> GaugeVec> = Lazy::new(|| {
	register_gauge_vec!(
		concat!(env!("CARGO_PKG_NAME"), "_query_time"),
		"Query execution time, in seconds",
		&["name"]
	)
	.unwrap()
});

#[derive(Copy, Clone)]
enum TimerType {
	Command,
	Query,
	Notification,
}

/// A timer for measuring and recording how long a command or database query took.
///
/// # Example
/// ```
/// async fn some_command() {
/// 	let _timer = Timer::commmand("commandname");
///
/// 	// command code goes here
///
/// } // _timer is dropped at the end of the function, recording the time elapsed since it was constructed
/// ```
pub struct Timer {
	kind: TimerType,
	name: &'static str,
	start: Instant,
}

impl Timer {
	/// Creates a timer for a command execution.
	///
	/// `name` should be the name of the command being executed.
	pub fn command(name: &'static str) -> Self {
		Self {
			kind: TimerType::Command,
			name,
			start: Instant::now(),
		}
	}

	/// Creates a timer for a database query execution.
	///
	/// `name` should be a brief description of the query, like `"delete keyword"`.
	pub fn query(name: &'static str) -> Self {
		Self {
			kind: TimerType::Query,
			name,
			start: Instant::now(),
		}
	}

	/// Creates a timer for a keyword notificationexecution.
	///
	/// `name` should be the type of notification, `"create"`, `"edit"`, or `"delete"`.
	pub fn notification(name: &'static str) -> Self {
		Self {
			kind: TimerType::Notification,
			name,
			start: Instant::now(),
		}
	}
}

impl Drop for Timer {
	/// Drop the timer, recording how long has elapsed since it was created.
	fn drop(&mut self) {
		if !ENABLED.get().unwrap() {
			return;
		}
		let elapsed = self.start.elapsed().as_secs_f64();

		match self.kind {
			TimerType::Command => {
				COMMAND_TIME_GAUGE
					.with_label_values(&[self.name])
					.set(elapsed);
			}
			TimerType::Query => {
				QUERY_TIME_GAUGE
					.with_label_values(&[self.name])
					.set(elapsed);
			}
			TimerType::Notification => {
				NOTIFY_TIME_GAUGE
					.with_label_values(&[self.name])
					.set(elapsed);
			}
		}
	}
}

/// Calculates average command execution time, in seconds.
///
/// This function calculates the average of the times of the most recent command usages. This is
/// not an average that accounts for how many commands were used, or how recently, it only goes
/// through each command and averages each of their most recent times. This is not a perfect
/// reflection of the actual average amount of time a command execution takes, but this is what is
/// recorded when using the prometheus library.
///
/// In the event that no command times have been recorded (such as if performance monitoring is
/// disabled) this function returns `None`.
pub fn avg_command_time() -> Option<f64> {
	avg_metrics(COMMAND_TIME_GAUGE.collect())
}

/// Calculates average database query execution time, in seconds.
///
/// This function calculates the average of the times of the most recent database queries. This is
/// not an average that accounts for how many queries were used, or how recently, it only goes
/// through each query and averages each of their most recent times. This is not a perfect
/// reflection of the actual average amount of time a database query takes, but this is what is
/// recorded when using the prometheus library.
///
/// In the event that no query times have been recorded (such as if performance monitoring is
/// disabled) this function returns `None`.
pub fn avg_query_time() -> Option<f64> {
	avg_metrics(QUERY_TIME_GAUGE.collect())
}

/// Calculates average keyword notification execution time, in seconds.
///
/// This function calculates the average of the times of the most recent keyword notification. This
/// is not an average that accounts for how many notifications were made, or how recently, it only
/// goes through each query and averages each of their most recent times. This is not a perfect
/// reflection of the actual average amount of time a keyword notification takes, but this is what
/// is recorded when using the prometheus library.
///
/// In the event that no notification times have been recorded (such as if performance monitoring is
/// disabled) this function returns `None`.
pub fn avg_notify_time() -> Option<f64> {
	avg_metrics(NOTIFY_TIME_GAUGE.collect())
}

/// Calculates the average of a collection of `MetricFamily`s.
fn avg_metrics(metric_families: Vec<MetricFamily>) -> Option<f64> {
	let mut count = 0;
	let mut sum = 0.0;
	for metric_family in metric_families {
		for metric in metric_family.get_metric() {
			sum += metric.get_gauge().get_value();
			count += 1;
		}
	}

	if count == 0 {
		None
	} else {
		Some(sum / count as f64)
	}
}

/// Initializes performance monitoring, starting a basic HTTP server for prometheus to poll.
pub fn init() {
	if let Some(addr) = &settings().logging.prometheus {
		match addr.to_socket_addrs() {
			Ok(mut sock_addr) => {
				ENABLED.set(true).unwrap();
				tokio::spawn(prometheus_server(
					sock_addr.next().expect("No socket address provided"),
				));
			}
			Err(e) => {
				ENABLED.set(false).unwrap();
				log::error!(
					"Prometheus address could not be resolve to IP: {}",
					e
				)
			}
		}
	} else {
		ENABLED.set(false).unwrap();

		log::warn!(
			"Prometheus address not provided; not starting monitoring server"
		);
	}
}

/// Runs the HTTP server for prometheus polling.
async fn prometheus_server(addr: SocketAddr) {
	let serve_future = Server::bind(&addr).serve(make_service_fn(|_| async {
		Ok::<_, hyper::Error>(service_fn(serve_req))
	}));

	if let Err(err) = serve_future.await {
		log::error!("Prometheus server error: {0}\n{0:?}", err);
	}
}

/// Responds to an HTTP request with all recorded performance metrics.
async fn serve_req(
	_req: Request<Body>,
) -> Result<Response<Body>, hyper::Error> {
	let encoder = TextEncoder::new();

	let metric_families = prometheus::gather();
	let mut buffer = vec![];
	encoder.encode(&metric_families, &mut buffer).unwrap();

	let response = Response::builder()
		.status(200)
		.header(CONTENT_TYPE, encoder.format_type())
		.body(Body::from(buffer))
		.unwrap();

	Ok(response)
}
