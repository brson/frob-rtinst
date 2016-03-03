#![allow(dead_code, unused_variables)]
#[macro_use]
extern crate log;
extern crate env_logger;
extern crate regex;
extern crate rustc_serialize;

use std::io::{self, BufReader, BufRead};
use std::time::Duration;
use event_log::Event;
use rustc_serialize::json;

mod event_parser;
mod event_log;
mod jsexport;

fn main() {
    env_logger::init().unwrap();

    let stream = BufReader::new(io::stdin());
    let ref mut parser = *event_log::event_parser();
    let events = stream.lines().filter_map(parser);
    let events = events.collect::<Vec<_>>();
    let boxes = build_mem_boxes(&events);

    let boxes: Vec<_> = boxes.iter().filter(|b| b.start_address != 1).cloned().collect();

    let min_address = boxes.iter().min_by_key(|b| b.start_address).unwrap();
    let max_address = boxes.iter().max_by_key(|b| b.end_address).unwrap();
    let min_address = min_address.start_address;
    let max_address = max_address.end_address;
    let min_time = boxes.iter().min_by_key(|b| b.start_time).unwrap();
    let max_time = boxes.iter().max_by_key(|b| b.end_time).unwrap();
    let min_time = min_time.start_time;
    let max_time = max_time.end_time;

    let metrics = Metrics {
        min_address: min_address,
        max_address: max_address,
        min_time: min_time,
        max_time: max_time,
    };

    let profile = Profile {
        metrics: metrics,
        mem_boxes: boxes.clone(),
    };

    let jsprofile = jsexport::convert(&profile);
    let encoded = json::encode(&jsprofile).unwrap();

    println!("{}", encoded);
}

#[derive(Debug)]
pub struct Profile {
    pub metrics: Metrics,
    pub mem_boxes: Vec<MemBox>,
}

#[derive(Debug, Copy, Clone)]
pub struct Metrics {
    pub min_address: u64,
    pub max_address: u64,
    pub min_time: Duration,
    pub max_time: Duration,
}

#[derive(Debug, Copy, Clone)]
pub struct MemBox {
    pub start_time: Duration,
    pub end_time: Duration,
    pub start_address: Address,
    pub end_address: Address,
    pub details: MemDetails,
}

#[derive(Debug, Copy, Clone)]
pub enum MemDetails {
    Allocation,
    Box,
    Rc,
    Arc,
    Vec { fill: u64 },
}

pub type Address = u64;

fn build_mem_boxes(events: &[Event]) -> Vec<MemBox> {
    use event_log::*;

    let mut boxes = Vec::new();

    let mut open_boxes = OpenBoxStack(Vec::new());
    
    for event in events {
        match event.details {
            EventDetails::Allocate { ptr, .. } => {
                open_boxes.assert_dont_know(ptr);
                open_boxes.push(OpenBox::Allocate(ptr), event);
            }
            EventDetails::Reallocate { inptr, old_size, size, outptr, .. } => {
                open_boxes.assert_dont_know(outptr);
                if let Some(prev_event) = open_boxes.pop(OpenBox::Allocate(inptr)) {
                    boxes.push(MemBox {
                        start_time: prev_event.timestamp,
                        end_time: event.timestamp,
                        start_address: inptr,
                        end_address: inptr + old_size,
                        details: MemDetails::Allocation,
                    });
                } else if let Some(prev_event) = open_boxes.pop(OpenBox::Reallocate(inptr)) {
                    boxes.push(MemBox {
                        start_time: prev_event.timestamp,
                        end_time: event.timestamp,
                        start_address: inptr,
                        end_address: inptr + old_size,
                        details: MemDetails::Allocation,
                    });
                } else {
                    error!("no open box for {:?}", event);
                }
                open_boxes.push(OpenBox::Reallocate(outptr), event);
            }
            EventDetails::Deallocate { ptr, old_size, .. } => {
                // rustc doesn't call drop for Box - it just deallocates directly,
                // so try to close open Box memboxes.
                if let Some(prev_event) = open_boxes.pop(OpenBox::BoxCreate(ptr)) {
                    boxes.push(MemBox {
                        start_time: prev_event.timestamp,
                        end_time: event.timestamp,
                        start_address: ptr,
                        end_address: ptr + old_size,
                        details: MemDetails::Box,
                    });
                }

                if let Some(prev_event) = open_boxes.pop(OpenBox::Allocate(ptr)) {
                    boxes.push(MemBox {
                        start_time: prev_event.timestamp,
                        end_time: event.timestamp,
                        start_address: ptr,
                        end_address: ptr + old_size,
                        details: MemDetails::Allocation,
                    });
                } else if let Some(prev_event) = open_boxes.pop(OpenBox::Reallocate(ptr)) {
                    boxes.push(MemBox {
                        start_time: prev_event.timestamp,
                        end_time: event.timestamp,
                        start_address: ptr,
                        end_address: ptr + old_size,
                        details: MemDetails::Allocation,
                    });
                } else {
                    error!("no open box for {:?}", event);
                }
            }
            EventDetails::BoxCreate { ptr, .. } => {
                open_boxes.assert_dont_know(ptr);
                open_boxes.push(OpenBox::BoxCreate(ptr), event);
            }
            EventDetails::BoxDrop { ref t, ptr } => {
                if let Some(prev_event) = open_boxes.pop(OpenBox::BoxCreate(ptr)) {
                    boxes.push(MemBox {
                        start_time: prev_event.timestamp,
                        end_time: event.timestamp,
                        start_address: ptr,
                        end_address: ptr + t.size,
                        details: MemDetails::Box,
                    });
                } else {
                    error!("no open box for {:?}", event);
                }
            }
            EventDetails::RcCreate { ptr, .. } => {
                open_boxes.assert_dont_know(ptr);
                open_boxes.push(OpenBox::RcCreate(ptr), event);
            }
            EventDetails::RcDrop { ptr, .. } => {
                if let Some(prev_event) = open_boxes.pop(OpenBox::RcCreate(ptr)) {
                    // RcDrop doesn't have a sized type so get the size
                    // from the RcCreate event
                    let t_size = match prev_event.details {
                        EventDetails::RcCreate { ref t, .. } => t.size,
                        _ => unreachable!(),
                    };
                    boxes.push(MemBox {
                        start_time: prev_event.timestamp,
                        end_time: event.timestamp,
                        start_address: ptr,
                        end_address: ptr + t_size,
                        details: MemDetails::Rc,
                    });
                } else {
                    error!("no open box for {:?}", event);
                }
            }
            EventDetails::ArcCreate { ptr, .. } => {
                open_boxes.assert_dont_know(ptr);
                open_boxes.push(OpenBox::ArcCreate(ptr), event);
            }
            EventDetails::ArcDrop { ptr, .. } => {
                if let Some(prev_event) = open_boxes.pop(OpenBox::ArcCreate(ptr)) {
                    // RcDrop doesn't have a sized type so get the size
                    // from the RcCreate event
                    let t_size = match prev_event.details {
                        EventDetails::ArcCreate { ref t, .. } => t.size,
                        _ => unreachable!(),
                    };
                    boxes.push(MemBox {
                        start_time: prev_event.timestamp,
                        end_time: event.timestamp,
                        start_address: ptr,
                        end_address: ptr + t_size,
                        details: MemDetails::Arc,
                    });
                } else {
                    error!("no open box for {:?}", event);
                }
            }
            EventDetails::VecCreate { ptr, .. } => {
                open_boxes.assert_dont_know(ptr);
                open_boxes.push(OpenBox::VecCreate(ptr), event);
            }
            EventDetails::VecResize { ref t, len, capacity, old_ptr, new_ptr } => {
                if let Some(prev_event) = open_boxes.pop(OpenBox::VecCreate(old_ptr)) {
                    boxes.push(MemBox {
                        start_time: prev_event.timestamp,
                        end_time: event.timestamp,
                        start_address: old_ptr,
                        end_address: old_ptr + t.size * capacity,
                        details: MemDetails::Vec { fill: t.size * len },
                    });
                } else if let Some(prev_event) = open_boxes.pop(OpenBox::VecResize(old_ptr)) {
                    boxes.push(MemBox {
                        start_time: prev_event.timestamp,
                        end_time: event.timestamp,
                        start_address: old_ptr,
                        end_address: old_ptr + t.size * capacity,
                        details: MemDetails::Vec { fill: t.size * len },
                    });
                } else {
                    error!("no open box for {:?}", event);
                }
                open_boxes.push(OpenBox::VecResize(new_ptr), event);
            }
            EventDetails::VecDrop { ref t, len, capacity, ptr } => {
                // FIXME: Vec::drop is periodically called with this
                // bogus pointer. I don't understand why.
                let bogus = 0x1d1d1d1d1d1d1d1d;
                if ptr == bogus { continue }

                if let Some(prev_event) = open_boxes.pop(OpenBox::VecCreate(ptr)) {
                    boxes.push(MemBox {
                        start_time: prev_event.timestamp,
                        end_time: prev_event.timestamp,
                        start_address: ptr,
                        end_address: ptr + t.size * capacity,
                        details: MemDetails::Vec { fill: t.size * len },
                    });
                } else if let Some(prev_event) = open_boxes.pop(OpenBox::VecResize(ptr)) {
                    boxes.push(MemBox {
                        start_time: prev_event.timestamp,
                        end_time: event.timestamp,
                        start_address: ptr,
                        end_address: ptr + t.size * capacity,
                        details: MemDetails::Vec { fill: t.size * len },
                    });
                } else {
                    error!("no open box for {:?}", event);
                }
            }
            _ => {
                error!("unhandled event: {:?}", event);
            }
        }
    }

    open_boxes.assert_empty();

    boxes
}

#[derive(PartialEq, Copy, Clone)]
enum OpenBox {
    Allocate(Address),
    Reallocate(Address),
    BoxCreate(Address),
    RcCreate(Address),
    ArcCreate(Address),
    VecCreate(Address),
    VecResize(Address),
}

struct OpenBoxStack<'a>(Vec<(OpenBox, &'a event_log::Event)>);

impl<'a> OpenBoxStack<'a> {
    fn push(&mut self, ob: OpenBox, e: &'a event_log::Event) {
        self.0.push((ob, e));
    }

    fn pop(&mut self, ob: OpenBox) -> Option<&'a event_log::Event> {
        let rev_index =
            self.0.iter()
            .rev()
            .enumerate()
            .find(|&(_, &(ob2, _))| ob == ob2)
            .map(|(rev_index, _)| rev_index);

        if let Some(rev_index) = rev_index {
            let index = self.0.len() - rev_index - 1;
            let (_, e) = self.0.remove(index);

            Some(e)
        } else {
            None
        }
    }

    fn assert_dont_know(&self, ptr: Address) {
    }

    fn assert_empty(&self) {
        for &(_, event) in &self.0 {
            println!("unclosed box: {:?}", event);
        }
    }
}

