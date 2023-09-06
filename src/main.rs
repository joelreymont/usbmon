use std::fmt;
use clap::Parser;
use rusb::UsbContext;
use std::sync::mpsc;

struct HotPlugHandler<T: rusb::UsbContext> {
    sender: mpsc::Sender<rusb::Device<T>>,
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone)]
enum Error {
    MissingSeparator,
    InvalidVID(String),
    InvalidPID(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::MissingSeparator => write!(f, "missing : separator"),
            Error::InvalidVID(s) => write!(f, "invalid hex VID {}", s),
            Error::InvalidPID(s) => write!(f, "invalid hex PID {}", s),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone)]
struct DeviceID {
    vid: u16,
    pid: u16,
}

impl fmt::Display for DeviceID {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:x}:{:x}", self.vid, self.pid)
    }
}

pub fn iterable_to_str<I, D>(iterable: I) -> String
where
    I: IntoIterator<Item = D>,
    D: fmt::Display,
{
    let mut iterator = iterable.into_iter();

    let head = match iterator.next() {
        None => return String::from("[]"),
        Some(x) => format!("[{}", x),
    };
    let body = iterator.fold(head, |a, v| format!("{}, {}", a, v));
    format!("{}]", body)
}

fn parse_device(arg: &str) -> Result<DeviceID> {
    let vec: Vec<&str> = arg.split(":").collect();
    if vec.len() < 2 {
        return Err(Error::MissingSeparator)
    }
    let vid = match u16::from_str_radix(vec[0], 16) {
        Err(_) => return Err(Error::InvalidVID(vec[0].to_string())),
        Ok(vid) => vid,
    };
    let pid = match u16::from_str_radix(vec[1], 16) {
        Err(_) => return Err(Error::InvalidPID(vec[1].to_string())),
        Ok(vid) => vid,
    };
    Ok(DeviceID{vid, pid})
}

impl<T: rusb::UsbContext> rusb::Hotplug<T> for HotPlugHandler<T> {
    fn device_arrived(&mut self, device: rusb::Device<T>) {
        _ = self.sender.send(device);
    }

    fn device_left(&mut self, device: rusb::Device<T>) {
        _ = self.sender.send(device);
    }
}

fn is_connected<T: rusb::UsbContext>(
    devices: rusb::Result<rusb::DeviceList<T>>, 
    ids: &Vec<DeviceID>
) -> Option<DeviceID> {
    match devices {
        Err(_) =>  None,
        Ok(devices) => {
            let result = devices
                .iter()
                .find(|dev| {
                    let desc = dev.device_descriptor().unwrap();
                    ids.iter().find(|id| desc.vendor_id() == id.vid && desc.product_id() == id.pid).is_some()
                });
            match result {
                Some(dev) => {
                    let desc = dev.device_descriptor().unwrap();
                    return Some(DeviceID{vid: desc.vendor_id(), pid: desc.product_id()})
                },
                None => None
            }
        },
    }
}

#[derive(Parser, Debug)]
#[command(version, long_about = None)]
struct Args {
   /// To watch for detach events
   #[arg(short, long)]
   detach: bool,

   /// Device id, vid:pid
   #[arg(short, long, num_args = 1.., value_parser=parse_device)]
   id: Vec<DeviceID>,

   /// Return immediately
   #[arg(short, long)]
   nowait: bool,

   /// Print out extra information
   #[arg(short, long)]
   verbose: bool,
}

fn main() -> rusb::Result<()> {
    let args = Args::parse();

    // check if device is already connected

    if args.verbose {
        let op = if args.detach { "detach" } else { "attach" };
        eprintln!("Waiting for {} to {}...", iterable_to_str(args.id.iter()), op);
    }

    let attach = !args.detach;

    let connected = is_connected(rusb::devices(), &args.id);
    if connected.is_some()  ^ !attach {
        if let Some(id) = connected {
            println!("{}", id);
        }
        return Ok(())
    }

    if args.nowait {
        return Err(rusb::Error::NoDevice)
    }

    if args.verbose {
        eprintln!("Waiting for USB events...");
    }

    // wait for device to be attached or detached

    if rusb::has_hotplug() {
        let ctx = rusb::Context::new()?;
        let (tx, rx) = mpsc::channel::<rusb::Device<rusb::Context>>();
        let mut reg = Some(
            rusb::HotplugBuilder::new()
                .enumerate(false)                                
                .register(&ctx, Box::new(HotPlugHandler{sender: tx}))?,
        );

        loop {
            if args.verbose {
                eprintln!("Loop...");
            }
            ctx.handle_events(None).unwrap();
            let dev = rx.recv().unwrap();
            let desc = dev.device_descriptor().unwrap();
            let connected = is_connected(ctx.devices(), &args.id);
            if args.verbose {
                eprintln!("Event from {:x}:{:x}, connected: {:?}", 
                    desc.vendor_id(), desc.product_id(), connected);
            }
            if connected.is_some() ^ !attach {
                if let Some(reg) = reg.take() {
                    ctx.unregister_callback(reg);
                    println!("{:x}:{:x}", desc.vendor_id(), desc.product_id());
                    break;
                }
            }
        }
        Ok(())
    } else {
        eprintln!("libusb hotplug api unsupported!");
        Ok(())
    }
}
