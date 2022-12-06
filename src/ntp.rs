use anyhow::{anyhow, Error};
use bytes::{Buf, BufMut};
use std::fmt;
use std::time::Duration;
use time::{macros::datetime, OffsetDateTime};

#[derive(Debug, Default, Clone)]
pub struct ShortTime(u16, u16);

impl From<ShortTime> for f64 {
    fn from(ShortTime(secs, frac): ShortTime) -> Self {
        secs as f64 + frac as f64 / 65536.0
    }
}

impl ShortTime {
    pub fn from_buf<B: Buf>(buf: &mut B) -> Self {
        Self(buf.get_u16(), buf.get_u16())
    }

    pub fn to_buf<B: BufMut>(&self, buf: &mut B) {
        buf.put_u16(self.0);
        buf.put_u16(self.1);
    }
}

#[derive(Debug, Default, Clone)]
pub struct Timestamp(u32, u32);

impl From<Timestamp> for OffsetDateTime {
    fn from(Timestamp(secs, frac): Timestamp) -> Self {
        datetime!(1900-01-01 00:00:00 UTC)
            + Duration::new(secs as _, (frac as f64 / 4.294967296) as _)
    }
}

impl Timestamp {
    pub fn from_buf<B: Buf>(buf: &mut B) -> Self {
        Self(buf.get_u32(), buf.get_u32())
    }

    pub fn to_buf<B: BufMut>(&self, buf: &mut B) {
        buf.put_u32(self.0);
        buf.put_u32(self.1);
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Leap {
    NoWarning,
    AddSecond,
    DelSecond,
    NotInSync,
}

impl TryFrom<u8> for Leap {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::NoWarning),
            1 => Ok(Self::AddSecond),
            2 => Ok(Self::DelSecond),
            3 => Ok(Self::NotInSync),
            _ => Err(anyhow!("illegal leap indicator value `{value}`")),
        }
    }
}

impl From<Leap> for u8 {
    fn from(value: Leap) -> Self {
        match value {
            Leap::NoWarning => 0,
            Leap::AddSecond => 1,
            Leap::DelSecond => 2,
            Leap::NotInSync => 3,
        }
    }
}

impl fmt::Display for Leap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let repr = match self {
            Self::NoWarning => "No Warning",
            Self::AddSecond => "Add Second",
            Self::DelSecond => "Delete Second",
            Self::NotInSync => "Not In Sync",
        };
        f.write_str(repr)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Mode {
    Unspecified,
    Active,
    Passive,
    Client,
    Server,
    Broadcast,
}

impl TryFrom<u8> for Mode {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Unspecified),
            1 => Ok(Self::Active),
            2 => Ok(Self::Passive),
            3 => Ok(Self::Client),
            4 => Ok(Self::Server),
            5 => Ok(Self::Broadcast),
            _ => Err(anyhow!("illegal mode value `{value}`")),
        }
    }
}

impl From<Mode> for u8 {
    fn from(value: Mode) -> Self {
        match value {
            Mode::Unspecified => 0,
            Mode::Active => 1,
            Mode::Passive => 2,
            Mode::Client => 3,
            Mode::Server => 4,
            Mode::Broadcast => 5,
        }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let repr = match self {
            Self::Unspecified => "Unspecified",
            Self::Active => "Active",
            Self::Passive => "Passive",
            Self::Client => "Client",
            Self::Server => "Server",
            Self::Broadcast => "Broadcast",
        };
        f.write_str(repr)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Poll(i8);

impl From<i8> for Poll {
    fn from(value: i8) -> Self {
        Self(value)
    }
}

impl fmt::Display for Poll {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if (6..=10).contains(&self.0) {
            write!(f, "{} seconds", 2u16.pow(self.0 as _))
        } else {
            write!(f, "invalid ({})", self.0)
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Precision(i8);

impl From<i8> for Precision {
    fn from(value: i8) -> Self {
        Self(value)
    }
}

impl fmt::Display for Precision {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:.9} seconds", 2.0_f64.powi(self.0 as _))
    }
}

#[derive(Debug, Default, Clone)]
pub struct Packet {
    pub lvm: u8,
    pub stratum: u8,
    pub poll: Poll,
    pub precision: Precision,
    pub root_delay: ShortTime,
    pub root_dispersion: ShortTime,
    pub ref_id: [u8; 4],
    pub reference_time: Timestamp,
    pub origin_time: Timestamp,
    pub receive_time: Timestamp,
    pub transmit_time: Timestamp,
}

impl Packet {
    pub fn new(leap: Leap, version: u8, mode: Mode) -> Self {
        Self {
            lvm: (u8::from(leap) << 6) | (version << 3) | u8::from(mode),
            stratum: 16,
            ..Self::default()
        }
    }

    pub fn from_buf<B: Buf>(buf: &mut B) -> Self {
        let lvm = buf.get_u8();
        let stratum = buf.get_u8();
        let poll = buf.get_i8().into();
        let precision = buf.get_i8().into();
        let root_delay = ShortTime::from_buf(buf);
        let root_dispersion = ShortTime::from_buf(buf);
        let mut ref_id = [0; 4];
        buf.copy_to_slice(&mut ref_id);

        Self {
            lvm,
            stratum,
            poll,
            precision,
            root_delay,
            root_dispersion,
            ref_id,
            reference_time: Timestamp::from_buf(buf),
            origin_time: Timestamp::from_buf(buf),
            receive_time: Timestamp::from_buf(buf),
            transmit_time: Timestamp::from_buf(buf),
        }
    }

    pub fn to_buf<B: BufMut>(&self, buf: &mut B) {
        buf.put_u8(self.lvm);
        buf.put_u8(self.stratum);
        buf.put_i8(self.poll.0);
        buf.put_i8(self.precision.0);
        self.root_delay.to_buf(buf);
        self.root_dispersion.to_buf(buf);
        buf.put_slice(&self.ref_id);
        self.reference_time.to_buf(buf);
        self.origin_time.to_buf(buf);
        self.receive_time.to_buf(buf);
        self.transmit_time.to_buf(buf);
    }

    pub fn leap_version_mode(&self) -> (Leap, u8, Mode) {
        (
            Leap::try_from(self.lvm >> 6).unwrap(),
            self.lvm >> 3 & 7,
            Mode::try_from(self.lvm & 7).unwrap(),
        )
    }
}
