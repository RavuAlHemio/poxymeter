use std::num::ParseIntError;

use clap::Clap;


#[derive(Clap, Debug)]
pub(crate) struct Opts {
    #[clap(short = 'v', long = "usb-vendor", default_value = "0x28e9", parse(try_from_str = try_parse_with_base))]
    pub usb_vendor: u16,

    #[clap(short = 'p', long = "usb-product", default_value = "0x028a", parse(try_from_str = try_parse_with_base))]
    pub usb_product: u16,

    #[clap(subcommand)]
    pub subcommand: Subcommand,
}


#[derive(Clap, Debug)]
pub(crate) enum Subcommand {
    ReadFile(ReadFileSubcommand),
    LiveData,
    SetDeviceId(SetDeviceIdSubcommand),
}


#[derive(Clap, Debug)]
pub(crate) struct ReadFileSubcommand {
    pub file_index: usize,
}


#[derive(Clap, Debug)]
pub(crate) struct SetDeviceIdSubcommand {
    pub device_id: String,
}


fn try_parse_with_base(mut num_str: &str) -> Result<u16, ParseIntError> {
    if num_str.starts_with("+") {
        num_str = &num_str[1..];
    }

    if num_str.starts_with("0x") {
        u16::from_str_radix(&num_str[2..], 16)
    } else if num_str.starts_with("0b") {
        u16::from_str_radix(&num_str[2..], 2)
    } else if num_str.starts_with("0o") {
        // who even uses octal anymore?
        u16::from_str_radix(&num_str[2..], 8)
    } else {
        u16::from_str_radix(num_str, 10)
    }
}
