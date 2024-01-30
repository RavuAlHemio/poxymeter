mod opts;
mod oximeter;


use std::collections::HashMap;

use clap::Clap;
use chrono::{Duration, NaiveDate, Local};
use hidapi::{HidApi, HidDevice};
use log::{self, debug, log_enabled};
use oximeter::RecordingMode;

use crate::opts::{Opts, Subcommand};
use crate::oximeter::{
    calculate_checksum, CommandCode, CommandQueue, INIT_BYTESTRING, is_checksum_ok,
    PropertyCode, receive_from_oximeter, send_to_oximeter,
};


fn handle_live(oxdev: &HidDevice, mut queue: &mut CommandQueue) {
    // enable data streaming
    let mut enable_streaming = Vec::with_capacity(3);
    enable_streaming.push(CommandCode::LiveDataCommand.into());
    enable_streaming.push(0x00); // also stream curve (ensures that the values arrive on time)
    enable_streaming.push(calculate_checksum(&enable_streaming));
    send_to_oximeter(&oxdev, &enable_streaming)
        .expect("failed to enable streaming on oximeter");

    println!("timestamp,pulse,spo2");

    // read, read, read
    let mut keepalive_counter: usize = 0;
    loop {
        receive_from_oximeter(&oxdev, &mut queue)
            .expect("failed to receive live data");

        while let Some(command) = queue.dequeue_command() {
            if is_checksum_ok(&command) {
                // ignore it
                continue;
            }

            if command.len() < 2 {
                // too short for our purposes
                continue;
            }
            if command[0] != CommandCode::LiveDataResponse.into() {
                // not what we're looking for
                continue;
            }
            if command[1] != 0x01 {
                // not the current readings
                continue;
            }
            if command.len() < 8 {
                // not the correct length for current readings
                continue;
            }

            // it's the current readings!
            let timestamp = Local::now();
            let pulse = command[3];
            let spo2 = command[4];
            println!("{} {} {}", timestamp.format("%Y-%m-%d %H:%M:%S"), pulse, spo2);
        }

        // send a keepalive every 8 messages
        keepalive_counter += 1;
        if keepalive_counter == 8 {
            let mut keepalive = Vec::with_capacity(2);
            keepalive.push(CommandCode::KeepAliveCommand.into());
            keepalive.push(calculate_checksum(&keepalive));
            send_to_oximeter(&oxdev, &enable_streaming)
                .expect("failed to send keepalive to oximeter");

            keepalive_counter = 0;
        }
    };
}

fn handle_read_auto(oxdev: &HidDevice, mut queue: &mut CommandQueue, file_index: usize) {
    {
        let mut count_command = Vec::with_capacity(3);
        count_command.push(CommandCode::GetAuxiliaryDataCommand.into());
        count_command.push(PropertyCode::AutoRecordedFiles.into());
        count_command.push(calculate_checksum(&count_command));
        send_to_oximeter(&oxdev, &count_command)
            .expect("failed to send count request");
    }

    receive_from_oximeter(&oxdev, &mut queue)
        .expect("failed to receive response to file count request");
    let mut pulse_count: usize = 0;
    let mut spo2_count: usize = 0;
    while let Some(response) = queue.dequeue_command() {
        if !is_checksum_ok(&response) {
            continue;
        }
        if response[0] != CommandCode::GetAuxiliaryDataResponse.into() {
            continue;
        }

        // topmost bit is not set, so shift up by 7
        pulse_count = (response[2] as usize) | ((response[3] as usize) << 7);
        spo2_count = (response[4] as usize) | ((response[5] as usize) << 7);

        break;
    }

    let file_count = pulse_count.min(spo2_count);

    if file_count == 0 {
        eprintln!("auto recording mode active and no files recorded");
        return;
    }

    if file_index > file_count {
        eprintln!("auto recording mode active, no file {} available (max {})", file_index, file_count);
        return;
    }

    // ask for metadata
    for i in 1..=file_count {
        {
            let mut advance_and_show_command = Vec::with_capacity(3);
            advance_and_show_command.push(CommandCode::AdvanceAndShowAutoRecordedFileHeaderCommand.into());
            advance_and_show_command.push(0x01); // advance by 1
            advance_and_show_command.push(calculate_checksum(&advance_and_show_command));
            send_to_oximeter(&oxdev, &advance_and_show_command)
                .expect("failed to send advance-and-show request");
        }

        receive_from_oximeter(&oxdev, &mut queue)
            .expect("failed to receive response to advance-and-show request");
        while let Some(response) = queue.dequeue_command() {
            if !is_checksum_ok(&response) {
                continue;
            }
            if response[0] != CommandCode::AdvanceAndShowAutoRecordedFileHeaderResponse.into() {
                continue;
            }

            let start_time = NaiveDate::from_ymd(
                (response[4] as i32) + 2000,
                response[5] as u32,
                response[6] as u32,
            ).and_hms(
                response[7] as u32,
                response[8] as u32,
                response[9] as u32
            );
            let this_file_length =
                (response[10] as usize)
                | ((response[11] as usize) << 7)
                | ((response[12] as usize) << 14)
            ;

            if i == file_index {
                // alright then, read the file!
                let mut mode_to_values: HashMap<u8, Vec<u8>> = HashMap::new();
                for mode in &[1, 2] {
                    let mut values = Vec::new();
                    let mut base_value = 0;
                    let mut base_value_top_nibble = false;

                    {
                        let mut get_file_command = Vec::with_capacity(9);
                        get_file_command.push(CommandCode::ReadAutoRecordedFileCommand.into());
                        get_file_command.push(0x04); // unknown constant
                        get_file_command.push(*mode);
                        get_file_command.push(0x01); // unknown constant
                        get_file_command.push(i.try_into().expect("file number too large"));
                        get_file_command.push(0x00);
                        get_file_command.push(0x00);
                        get_file_command.push(0x00);
                        get_file_command.push(calculate_checksum(&get_file_command));
                        send_to_oximeter(&oxdev, &get_file_command)
                            .expect("failed to send read-auto-file request");
                    }

                    while values.len() < this_file_length {
                        // we have more data to fetch
                        receive_from_oximeter(&oxdev, &mut queue)
                            .expect("failed to receive response to read-auto-file request");
                        while let Some(response) = queue.dequeue_command() {
                            if !is_checksum_ok(&response) {
                                continue;
                            }
                            if response[0] != CommandCode::ReadAutoRecordedFileResponse.into() {
                                continue;
                            }
                            if response.len() != 30 {
                                continue;
                            }

                            // again, since the top bit may not be set except at the beginning of a command,
                            // the sign bits for the top nibbles have been moved to the front
                            let sign_bits
                                = (response[5] as u32)
                                | ((response[6] as u32) << 7)
                                | ((response[7] as u32) << 14)
                                ;

                            let mut debug_all_bytes = Vec::new();
                            for (j, b) in response[8..29].iter().enumerate() {
                                // top nibble needs the additional bit from the sign_bits
                                let mut top_nibble = (*b >> 4) & 0x0F;
                                if sign_bits & (1 << j) != 0 {
                                    top_nibble |= 0b1000;
                                }

                                // bottom nibble does not
                                let bottom_nibble = (*b >> 0) & 0x0F;

                                debug_all_bytes.push(top_nibble << 4 | bottom_nibble);

                                if top_nibble == 0x0F {
                                    if bottom_nibble == 0x0F && !base_value_top_nibble {
                                        // invalid value
                                        // (unless we are waiting for the bottom nibble of the new base value)
                                        values.push(0xFF);
                                        values.push(0xFF);
                                        continue;
                                    }

                                    // we are changing the base value!
                                    if base_value_top_nibble {
                                        base_value |= bottom_nibble;
                                        base_value_top_nibble = false;
                                    } else {
                                        base_value = bottom_nibble << 4;
                                        base_value_top_nibble = true;
                                    }

                                    // note that this does not generate a value
                                } else {
                                    // the nibbles are (downward) deltas from the current base value
                                    values.push(base_value - top_nibble);
                                    if bottom_nibble != 0x0F {
                                        // 0x0F is invalid
                                        values.push(base_value - bottom_nibble);
                                    }

                                    if values.len() == this_file_length {
                                        // we are done
                                        break;
                                    }
                                }
                            }

                            if log_enabled!(log::Level::Debug) {
                                let bstrs: Vec<String> = debug_all_bytes.iter()
                                    .map(|b| format!("{:02x}", b))
                                    .collect();
                                debug!("DATA IS {}", bstrs.join(" "));
                            }
                        }
                    }

                    // and we're done
                    mode_to_values.insert(*mode, values);
                }

                // zip the values together and output them
                let spo2_values = &mode_to_values[&1];
                let pulse_values = &mode_to_values[&2];
                let mut cur_time = start_time;
                println!("timestamp,pulse,spo2");
                for (spo2, pulse) in spo2_values.iter().zip(pulse_values.iter()) {
                    println!("{},{},{}", cur_time.format("%Y-%m-%d %H:%M:%S"), *pulse, *spo2);
                    cur_time += Duration::seconds(1);
                }
            }

            break;
        }
    }
}

fn handle_read_manual(oxdev: &HidDevice, mut queue: &mut CommandQueue, file_index: usize) {
    if file_index != 1 {
        eprintln!("manual recording mode active, file index must be 1");
        return;
    }

    {
        let mut metadata_command = Vec::with_capacity(3);
        metadata_command.push(CommandCode::ManuallyRecordedFileMetadataCommand.into());
        metadata_command.push(0x00); // there is only one file
        metadata_command.push(calculate_checksum(&metadata_command));
        send_to_oximeter(&oxdev, &metadata_command)
            .expect("failed to send metadata request");
    }

    let (start_time, this_file_length) = loop {
        receive_from_oximeter(&oxdev, &mut queue)
            .expect("failed to receive response to file metadata request");
        let mut start_time = None;
        let mut this_file_length = None;
        while let Some(response) = queue.dequeue_command() {
            if !is_checksum_ok(&response) {
                continue;
            }
            if response.len() < 2 {
                continue;
            }
            if response[0] != CommandCode::ManuallyRecordedFileMetadataResponse.into() {
                continue;
            }
            if response.len() != 14 {
                continue;
            }

            let file_length
                = (response[10] as usize)
                | ((response[11] as usize) << 7)
                | ((response[12] as usize) << 14)
                ;

            if file_length == 0 {
                eprintln!("no file recorded in manual recording mode");
                return;
            }

            // round length down to a multiple of 27
            let full_chunk_count = file_length / 27;

            start_time = Some(
                NaiveDate::from_ymd(
                    (response[2] as i32) + 2000,
                    response[3] as u32,
                    response[4] as u32,
                ).and_hms(
                    response[5] as u32,
                    response[6] as u32,
                    response[7] as u32
                )
            );
            this_file_length = Some(full_chunk_count * 27);
        }

        if let Some(st) = start_time {
            if let Some(tfl) = this_file_length {
                break (st, tfl);
            }
        }
    };

    let read_commands_responses = &[
        (
            CommandCode::ReadPulseFromManuallyRecordedFileCommand,
            CommandCode::ReadPulseFromManuallyRecordedFileResponse,
        ),
        (
            CommandCode::ReadOxygenFromManuallyRecordedFileCommand,
            CommandCode::ReadOxygenFromManuallyRecordedFileResponse,
        ),
    ];

    // list of lists of values (Gollum English)
    let mut valueses = Vec::new();
    for (read_command, read_response) in read_commands_responses {
        // read the file!
        {
            let mut read_pulse_command = Vec::with_capacity(5);
            read_pulse_command.push(read_command.into());
            read_pulse_command.push(0x00);
            read_pulse_command.push(0x00);
            read_pulse_command.push(0x00);
            read_pulse_command.push(calculate_checksum(&read_pulse_command));
            send_to_oximeter(&oxdev, &read_pulse_command)
                .expect("failed to send read-pulse request");
        }

        let mut values = Vec::new();
        loop {
            receive_from_oximeter(&oxdev, &mut queue)
                .expect("failed to receive read-pulse request");
            while let Some(response) = queue.dequeue_command() {
                if !is_checksum_ok(&response) {
                    continue;
                }
                if response.len() < 2 {
                    continue;
                }
                if response[0] != read_response.into() {
                    continue;
                }
                if response.len() != 20 {
                    continue;
                }

                // once more, the topmost bits have been "outsourced"
                let signs
                    = (response[3] as u16)
                    | ((response[4] as u16) << 7)
                    ;
                let mut value_byte = response[5];
                // the signs also contain the sign for the value byte
                if signs & 1 != 0 {
                    value_byte |= 0b1000_0000;
                }

                // this base value is also part of the output!
                values.push(value_byte);

                for (i, b) in response[6..19].iter().enumerate() {
                    // take top nibble sign from signs
                    let mut top_nibble = (*b >> 4) & 0x0F;
                    // (adding 1 to the left-shift because 0 is used for the initial value byte)
                    if signs & (1 << (i + 1)) != 0 {
                        top_nibble |= 0b1000;
                    }
                    let bottom_nibble = *b & 0x0F;

                    for nibble in [top_nibble, bottom_nibble] {
                        // TODO: handle 0xF nibble as an invalid value
                        if nibble == 0xF {
                            value_byte = 0xFF;
                        } else if nibble & 0b1000 != 0 {
                            // subtract from base value
                            value_byte -= nibble & 0b0111;
                        } else {
                            // add to base value
                            value_byte += nibble & 0b0111;
                        }

                        values.push(value_byte);
                    }
                }
            }

            debug!("values.len(): {}, this_file_length: {}", values.len(), this_file_length);
            if values.len() >= this_file_length {
                values.truncate(this_file_length);
                break;
            }
        }
        valueses.push(values);
    }

    // merge the values and output as CSV
    let pulse_values = &valueses[0];
    let spo2_values = &valueses[1];
    let mut cur_time = start_time;
    println!("timestamp,pulse,spo2");
    for (spo2, pulse) in spo2_values.iter().zip(pulse_values.iter()) {
        println!("{},{},{}", cur_time.format("%Y-%m-%d %H:%M:%S"), *pulse, *spo2);
        cur_time += Duration::seconds(1);
    }
}

fn handle_read_file(oxdev: &HidDevice, mut queue: &mut CommandQueue, file_index: usize) {
    if file_index == 0 {
        eprintln!("file 0 does not exist");
    }

    // check if auto or manual
    let mut rec_mode_command = Vec::with_capacity(3);
    rec_mode_command.push(CommandCode::ReadPropertyCommand.into());
    rec_mode_command.push(PropertyCode::RecordingMode.into());
    rec_mode_command.push(calculate_checksum(&rec_mode_command));
    send_to_oximeter(&oxdev, &rec_mode_command)
        .expect("failed to send recording mode request");

    let rec_mode = loop {
        receive_from_oximeter(&oxdev, &mut queue)
            .expect("failed to receive response to file metadata request");
        let mut rm = None;
        while let Some(response) = queue.dequeue_command() {
            if !is_checksum_ok(&response) {
                continue;
            }
            if response.len() < 2 {
                continue;
            }
            if response[0] != CommandCode::ReadPropertyResponse.into() {
                continue;
            }
            if response[1] != PropertyCode::RecordingMode.into() {
                continue;
            }
            if response.len() != 5 {
                continue;
            }

            let rec_mode_code
                = (response[2] as u16)
                | ((response[3] as u16) << 7)
                ;
            rm = Some(RecordingMode::from(rec_mode_code));
        }

        if let Some(concrete_rm) = rm {
            break concrete_rm;
        }
    };

    match rec_mode {
        RecordingMode::Automatic => handle_read_auto(oxdev, queue, file_index),
        RecordingMode::Manual => handle_read_manual(oxdev, queue, file_index),
        RecordingMode::Other(o) => panic!("unknown recording mode {}", o),
    }
}

fn handle_set_device_id(oxdev: &HidDevice, mut queue: &mut CommandQueue, device_id: &str) {
    let mut device_id_bytes: Vec<u8> = device_id.bytes().collect();
    if device_id_bytes.len() > 7 {
        panic!("device ID cannot be longer than 7 bytes");
    }
    if device_id_bytes.iter().any(|b| *b > 0x7F) {
        panic!("device ID cannot contain bytes above 0x7F");
    }
    while device_id_bytes.len() < 7 {
        // right-pad with spaces
        device_id_bytes.push(0x20);
    }

    // set it!
    {
        let mut set_command = Vec::with_capacity(10);
        set_command.push(CommandCode::SetPropertyCommand.into());
        set_command.push(PropertyCode::DeviceId.into());
        set_command.extend_from_slice(&device_id_bytes);
        set_command.push(calculate_checksum(&set_command));
        send_to_oximeter(&oxdev, &set_command)
            .expect("failed to set device ID on oximeter");

        loop {
            receive_from_oximeter(&oxdev, &mut queue)
                .expect("failed to obtain set-device-ID response from oximeter");
            while let Some(response) = queue.dequeue_command() {
                if !is_checksum_ok(&response) {
                    continue;
                }
                if response.len() < 2 {
                    continue;
                }
                if response[0] != CommandCode::SetPropertyResponse.into() {
                    continue;
                }

                return;
            }
        }
    }
}


fn main() {
    env_logger::init();

    let opts = Opts::parse();

    let hidapi = HidApi::new()
        .expect("failed to instantiate HidApi");

    let oxdev = hidapi.open(opts.usb_vendor, opts.usb_product)
        .expect("failed to open oximeter device");

    // write init string
    send_to_oximeter(&oxdev, INIT_BYTESTRING)
        .expect("failed to send init string to oximeter");

    // read response
    let mut queue = CommandQueue::new();
    receive_from_oximeter(&oxdev, &mut queue)
        .expect("failed to obtain init response from oximeter");
    let init_expected = Some(vec![0xf0, 0x70]);
    let init_response = queue.dequeue_command();
    if init_response != init_expected {
        panic!("wrong init response: expected {:?}, obtained {:?}", init_expected, init_response);
    }

    match opts.subcommand {
        Subcommand::LiveData => handle_live(&oxdev, &mut queue),
        Subcommand::ReadFile(read_file) => handle_read_file(&oxdev, &mut queue, read_file.file_index),
        Subcommand::SetDeviceId(u) => handle_set_device_id(&oxdev, &mut queue, &u.device_id),
    };
}
