#[macro_use]
extern crate log;
extern crate env_logger;
extern crate regex;

use std::io::{self, BufReader, BufRead};
use regex::Regex;

fn main() {
    env_logger::init().unwrap();

    let stream = BufReader::new(io::stdin());
    let ref mut parser = *event_parser();
    let events = stream.lines().filter_map(parser);
    for _ in events { }
}

#[derive(Debug)]
struct Event {
    thread: u64,
    timestamp: Duration,
    details: EventDetails,
}

// This is copy-pasted from rtinst/lib, but with all the pointers and
// u64s replaced by u64s
#[derive(Debug)]
#[allow(dead_code)]
enum EventDetails {
    // Alllocator
    Allocate { size: u64, align: u64, ptr: u64 },
    Reallocate { inptr: u64, old_size: u64, size: u64, align: u64, outptr: u64 },
    ReallocateInplace { ptr: u64, old_size: u64, size: u64, align: u64 },
    Deallocate { ptr: u64, old_size: u64, align: u64 },

    // Box
    BoxCreate { t: TypeInfo, ptr: u64 },
    BoxDrop { t: TypeInfo, ptr: u64 },

    // Rc
    RcCreate { t: TypeInfo, ptr: u64 },
    RcDrop { t: UnsizedTypeInfo, ptr: u64 },

    // Arc
    ArcCreate { t: TypeInfo, ptr: u64 },
    ArcDrop { t: UnsizedTypeInfo, ptr: u64 },

    // Vec
    VecCreate { t: TypeInfo, len: u64, capacity: u64, ptr: u64 },
    VecResize { t: TypeInfo, len: u64, capacity: u64, old_ptr: u64, new_ptr: u64 },
    VecDrop { t: TypeInfo, len: u64, capacity: u64, ptr: u64 },
}

#[derive(Debug)]
struct TypeInfo {
    name: String,
    size: usize,
}

#[derive(Debug)]
struct UnsizedTypeInfo {
    name: String,
}

fn event_parser() -> Box<FnMut(io::Result<String>) -> Option<Event>> {
    let regex = r"^RTINST \[(\d*)\]\[(\d*).(\d*)\] (.*)$";
    let regex = Regex::new(regex).unwrap();
    Box::new(move |line: io::Result<String>| {
        parse_event(&regex, &line.unwrap())
    })
}

fn parse_event(regex: &Regex, line: &str) -> Option<Event> {
    if let Some(cap) = regex.captures(line) {
        let thread_id: u64 = cap.at(1).unwrap().parse().unwrap();
        let secs: u64 = cap.at(2).unwrap().parse().unwrap();
        let ns: u64 = cap.at(3).unwrap().parse().unwrap();
        let event = parse_details(&cap.at(4).unwrap());
        if let Some(event) = event {
            Some(Event {
                thread: thread_id,
                details: EventDetails::Allocate { size: 0, align: 0, ptr: 0 }
            })
        } else {
            error!("event doesn't parse: {}", line);
            None
        }
    } else {
        error!("line doesn't match regex: {}", line);
        None
    }
}

fn parse_details(s: &str) -> Option<EventDetails> {
    None
}
