//! Canonical launch-profile and launch-source codecs.

use crate::{
    MAX_WORKBENCH_LAUNCH_ARGUMENTS, MAX_WORKBENCH_LAUNCH_ENVIRONMENT, WorkbenchBuildIdentity,
    WorkbenchLaunchProfile, WorkbenchLaunchSource, WorkbenchLaunchSourceKind,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_types::RunId;

use super::{read_count, read_text, write_count};
use crate::wire::primitive::{invalid, read_digest, read_id, write_digest, write_id};

pub(in crate::wire) fn write_profile(
    writer: &mut CanonicalWriter,
    value: &WorkbenchLaunchProfile,
) -> Result<(), CodecError> {
    write_id(writer, value.run().as_bytes())?;
    writer.write_str(value.executable().as_str())?;
    write_count(writer, value.arguments().len())?;
    for argument in value.arguments() {
        writer.write_str(argument.as_str())?;
    }
    writer.write_str(value.working_directory().as_str())?;
    write_count(writer, value.environment().len())?;
    for name in value.environment() {
        writer.write_str(name.as_str())?;
    }
    write_source(writer, value.source())?;
    writer.write_bool(value.build().is_some())?;
    if let Some(build) = value.build() {
        writer.write_str(build.path().as_str())?;
        write_digest(writer, build.digest())?;
    }
    writer.write_u64(value.readiness_millis())?;
    writer.write_u64(value.wall_millis())?;
    writer.write_bool(value.interactive())?;
    writer.write_u16(1)?; // inherited-host network posture
    writer.write_u16(1) // terminate-owned-tree stop policy
}

pub(in crate::wire) fn read_profile(
    reader: &mut CanonicalReader<'_>,
) -> Result<WorkbenchLaunchProfile, CodecError> {
    let offset = reader.offset();
    let run = read_id(reader, RunId::new)?;
    let executable = read_text(reader)?;
    let argument_count = read_count(reader, MAX_WORKBENCH_LAUNCH_ARGUMENTS)?;
    let mut arguments = Vec::with_capacity(argument_count);
    for _ in 0..argument_count {
        arguments.push(read_text(reader)?);
    }
    let working_directory = read_text(reader)?;
    let environment_count = read_count(reader, MAX_WORKBENCH_LAUNCH_ENVIRONMENT)?;
    let mut environment = Vec::with_capacity(environment_count);
    for _ in 0..environment_count {
        environment.push(read_text(reader)?);
    }
    let source = read_source(reader)?;
    let build = if reader.read_bool()? {
        Some(WorkbenchBuildIdentity::new(read_text(reader)?, read_digest(reader)?))
    } else {
        None
    };
    let readiness = reader.read_u64()?;
    let wall = reader.read_u64()?;
    let interactive = reader.read_bool()?;
    let network_offset = reader.offset();
    if reader.read_u16()? != 1 {
        return Err(CodecError::at(CodecErrorKind::UnknownTag, network_offset));
    }
    let stop_offset = reader.offset();
    if reader.read_u16()? != 1 {
        return Err(CodecError::at(CodecErrorKind::UnknownTag, stop_offset));
    }
    invalid(
        offset,
        WorkbenchLaunchProfile::new(
            run,
            executable,
            arguments,
            working_directory,
            environment,
            source,
            build,
            readiness,
            wall,
            interactive,
        ),
    )
}

fn write_source(
    writer: &mut CanonicalWriter,
    value: &WorkbenchLaunchSource,
) -> Result<(), CodecError> {
    writer.write_u16(value.kind().tag())?;
    writer.write_str(value.path().as_str())?;
    write_digest(writer, value.digest())
}

fn read_source(reader: &mut CanonicalReader<'_>) -> Result<WorkbenchLaunchSource, CodecError> {
    let offset = reader.offset();
    let kind = WorkbenchLaunchSourceKind::from_tag(reader.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, offset))?;
    Ok(WorkbenchLaunchSource::new(kind, read_text(reader)?, read_digest(reader)?))
}
