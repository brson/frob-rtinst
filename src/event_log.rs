use std::io;
use std::time::Duration;
use regex::Regex;
use event_parser;

#[derive(Debug)]
pub struct Event {
    pub thread: u64,
    pub timestamp: Duration,
    pub details: EventDetails,
}

// This is copy-pasted from rtinst/lib, but with all the pointers and
// u64s replaced by u64s
#[derive(Debug)]
#[allow(dead_code)]
pub enum EventDetails {
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
pub struct TypeInfo {
    pub name: String,
    pub size: u64,
}

#[derive(Debug)]
pub struct UnsizedTypeInfo {
    pub name: String,
}

pub fn event_parser() -> Box<FnMut(io::Result<String>) -> Option<Event>> {
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
        let ns: u32 = cap.at(3).unwrap().parse().unwrap();
        let event = parse_details(&cap.at(4).unwrap());
        if let Some(event) = event {
            Some(Event {
                thread: thread_id,
                timestamp: Duration::new(secs, ns),
                details: event,
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
    event_parser::parse_EventDetails(s).ok()
}
