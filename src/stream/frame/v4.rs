use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use std::io::{Read, Write};
use frame::Frame;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use ::tag;
use ::stream::encoding::Encoding;
use ::stream::frame;
use ::stream::unsynch;

pub fn decode(reader: &mut Read) -> ::Result<Option<(usize, Frame)>> {
    let id = id_or_padding!(reader, 4);
    let mut frame = Frame::new(id);
    debug!("reading {}", frame.id());

    let content_size = unsynch::decode_u32(reader.read_u32::<BigEndian>()?);

    let frameflags = reader.read_u16::<BigEndian>()?;
    frame.flags.tag_alter_preservation = frameflags & 0x4000 != 0;
    frame.flags.file_alter_preservation = frameflags & 0x2000 != 0;
    frame.flags.read_only = frameflags & 0x1000 != 0;
    frame.flags.grouping_identity = frameflags & 0x40 != 0;
    frame.flags.compression = frameflags & 0x08 != 0;
    frame.flags.encryption = frameflags & 0x04 != 0;
    frame.flags.unsynchronization = frameflags & 0x02 != 0;
    frame.flags.data_length_indicator = frameflags & 0x01 != 0;

    if frame.flags.encryption {
        debug!("[{}] encryption is not supported", frame.id());
        return Err(::Error::new(::ErrorKind::UnsupportedFeature, "encryption is not supported"));
    } else if frame.flags.grouping_identity {
        debug!("[{}] grouping identity is not supported", frame.id());
        return Err(::Error::new(::ErrorKind::UnsupportedFeature, "grouping identity is not supported"));
    }

    let mut read_size = content_size;
    if frame.flags.data_length_indicator {
        let _decompressed_size = unsynch::decode_u32(reader.read_u32::<BigEndian>()?);
        read_size -= 4;
    }
    frame.content = super::decode_content(reader.take(read_size as u64), frame.id(), frame.flags)?;

    Ok(Some((10 + content_size as usize, frame)))
}

pub fn write(writer: &mut Write, frame: &Frame) -> ::Result<u32> {
    let mut content_bytes = frame::content_to_bytes(&frame, tag::Id3v24, Encoding::UTF8);
    let mut content_size = content_bytes.len() as u32;
    let decompressed_size = content_size;

    if frame.flags.compression {
        debug!("[{}] compressing frame content", frame.id());
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::Default);
        try!(encoder.write_all(&content_bytes[..]));
        content_bytes = try!(encoder.finish());
        content_size = content_bytes.len() as u32;
    }

    if frame.flags.data_length_indicator {
        content_size += 4;
    }

    try!(writer.write_all(frame.id().as_bytes()));
    try!(writer.write_u32::<BigEndian>(unsynch::encode_u32(content_size)));;
    try!(writer.write_all(&frame.flags.to_bytes(0x4)[..]));
    if frame.flags.data_length_indicator {
        debug!("[{}] adding data length indicator", frame.id());
        try!(writer.write_u32::<BigEndian>(unsynch::encode_u32(decompressed_size)));
    }
    if frame.flags.unsynchronization {
        unsynch::encode_vec(&mut content_bytes);
    }
    try!(writer.write_all(&content_bytes[..]));

    Ok(10 + content_size)
}
