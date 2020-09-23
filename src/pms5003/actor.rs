use super::Frame;
use actix::fut::wrap_future;
use actix::prelude::*;
use log::*;
use std::ops::Add;
use std::time::SystemTime;

pub struct Monitor {
    last_frame: Option<Frame>,

    last_5m_slot: Option<usize>,
    slots_5m: Vec<Option<FrameStats>>,
}

#[derive(Debug, Clone)]
struct FrameStats {
    samples: u32,
    pm10_standard_sum: u32,
    pm25_standard_sum: u32,
}

impl FrameStats {
    pub fn add_frame(mut self, frame: &Frame) -> Self {
        self.samples += 1;
        self.pm10_standard_sum += frame.pm10_standard as u32;
        self.pm25_standard_sum += frame.pm25_standard as u32;
        self
    }

    pub fn from_frame(frame: &Frame) -> Self {
        FrameStats {
            samples: 1,
            pm10_standard_sum: frame.pm10_standard as u32,
            pm25_standard_sum: frame.pm25_standard as u32,
        }
    }
}

impl Add for FrameStats {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        FrameStats {
            samples: self.samples + other.samples,
            pm10_standard_sum: self.pm10_standard_sum + other.pm10_standard_sum,
            pm25_standard_sum: self.pm25_standard_sum + other.pm25_standard_sum,
        }
    }
}

impl Monitor {
    pub fn new() -> Self {
        let slots_5m = vec![None; 12];
        Monitor {
            last_frame: None,
            last_5m_slot: None,
            slots_5m,
        }
    }
}

impl Actor for Monitor {
    type Context = Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        let addr = ctx.address();

        info!("Monitor actor started - spawning monitor thread");
        ctx.spawn(wrap_future(async move {
            let addr = addr.clone();
            super::monitor(addr, "/dev/ttyAMA0").await;
        }));
        info!("monitor spawned")
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct NewFrame(pub Frame);

#[derive(Message)]
#[rtype(result = "Option<super::Frame>")]
pub struct GetLastFrame;

impl Handler<NewFrame> for Monitor {
    type Result = ();

    fn handle(&mut self, msg: NewFrame, _ctx: &mut Self::Context) -> Self::Result {
        let frame = msg.0;
        info!("{:?}", frame);

        // When a frame comes in, we track it based on 5m, 60m, and 24h running averages.  To do
        // this, we need to keep track of what slot this will go into, whether it's different than
        // the last time (to trigger rotation), and recompute.

        let now = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Resolve to a rotating index 0-11
        let this_5m_slot = (now / 300 % 12) as usize;

        if self.last_5m_slot != Some(this_5m_slot) {
            // Initialize or purge this slot
            self.slots_5m[this_5m_slot] = Some(FrameStats::from_frame(&frame));
        } else {
            // Augment the slot by dropping in a fresh one
            self.slots_5m[this_5m_slot] = match &self.slots_5m[this_5m_slot] {
                Some(stats) => Some(stats.clone().add_frame(&frame)),
                None => panic!("now what?"),
            };
        }

        self.last_frame = Some(frame);
        self.last_5m_slot = Some(this_5m_slot);
        info!("5m-slots = {:#?}", self.slots_5m);
    }
}

impl Handler<GetLastFrame> for Monitor {
    type Result = Option<super::Frame>;

    fn handle(&mut self, _msg: GetLastFrame, _ctx: &mut Self::Context) -> Self::Result {
        self.last_frame.clone()
    }
}
