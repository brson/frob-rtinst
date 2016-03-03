#[macro_use]
extern crate log;
extern crate env_logger;
extern crate regex;

use std::io::{self, BufReader, BufRead};
use std::time::Duration;

mod event_parser;
mod event_log;

fn main() {
    env_logger::init().unwrap();

    let stream = BufReader::new(io::stdin());
    let ref mut parser = *event_log::event_parser();
    let events = stream.lines().filter_map(parser);
    let events = events.collect::<Vec<_>>();
    let boxes = build_mem_boxes(&events);
}

struct MemBox {
    p1: MemPoint,
    p2: MemPoint,
}

enum MemDetails {
    Allocation,
    Box,
    Rc,
    Arc,
    Vec { len: u64 },
}

type Address = u64;
struct MemPoint(Duration, Address);

fn build_mem_boxes(events: &[event_log::Event]) -> Vec<MemBox> {
    unimplemented!()
}
