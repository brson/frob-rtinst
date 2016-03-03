#![allow(dead_code, unused_variables)]
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

    for box_ in boxes {
        println!("box: {:?}", box_);
    }
}

#[derive(Debug)]
struct MemBox {
    start_time: Duration,
    end_time: Duration,
    start_address: Address,
    end_address: Address,
    details: MemDetails,
}

#[derive(Debug)]
enum MemDetails {
    Allocation,
    Box,
    Rc,
    Arc,
    VecCapacity,
    VecLen,
}

type Address = u64;

fn build_mem_boxes(events: &[event_log::Event]) -> Vec<MemBox> {
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
