//! Fenced command cursor over the production E0 reducer.

use peritus_codec::{CodecLimits, decode_message};
use peritus_types::{CommandId, EventId};

use crate::{
    OrchestratorCommand, OrchestratorCommandFrame, OrchestratorCommandKind, OrchestratorState,
    OrchestratorTransition,
};

const GENESIS_COMMAND_HEX: [&str; 17] = [
    "505254530001004c00010000000006030101010101010101010101010101010102020202020202020202020202020202373737373737373737373737373737370000000000000000000000000000000000000000000000000000000000000000",
    "0000000000000000000a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a2323232323232323232323232323232324242424242424242424242424242424000000000000000100000000000000012525252525252525252525252525252526262626262626",
    "262626262626262626013838383838383838383838383838383837373737373737373737373737373737393939393939393939393939393939390a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a14141414141414141414141414141414141414141414",
    "141414141414141414140a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a23232323232323232323232323232323242424242424242424242424242424240000000000000001000000000000000125252525252525252525252525252525262626262626",
    "262626262626262626263a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c000200043d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3e3e",
    "3e3e3e3e3e3e3e3e3e3e3e3e3e3e3e3e3e3e3e3e3e3e3e3e3e3e3e3e3e3e3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f4040404040404040404040404040404040404040404040404040404040404040414141414141414141414141414141414242",
    "42424242424242424242424242424242424242424242424242424242424200080004000c001000140020003000400060008000000000001000000000000000200000b8d47e258b83834af89593b8518f700a6b95638ac7740f9caee4157c42eb",
    "c0940a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a232323232323232323232323232323232424242424242424242424242424242400000000000000010000000000000001252525252525252525252525252525252626262626262626262626262626",
    "26262c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2f2f2f2f2f2f2f2f2f2f2f2f2f2f",
    "2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f00000000000129292929292929292929292929292929000000013030303030303030303030303030303030303030303030303030303030303030f93fcfd2a3e75fb68d6a6d30db8911888e48178c",
    "6f4e836e851b6bc37d23cbbe28282828282828282828282828282828062929292929292929292929292929292901012a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a0203000000012b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b0302313131313131313131",
    "31313131313131010001012828282828282828282828282828282829292929292929292929292929292929020a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a232323232323232323232323232323232424242424242424242424242424242400000000",
    "00000001000000000000000125252525252525252525252525252525262626262626262626262626262626262c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2e2e2e2e",
    "2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f000000000001292929292929292929292929292929290000000130303030303030303030",
    "30303030303030303030303030303030303030303030f93fcfd2a3e75fb68d6a6d30db8911888e48178c6f4e836e851b6bc37d23cbbe013232323232323232323232323232323233333333333333333333333333333333343434343434343434",
    "34343434343434000000013535353535353535353535353535353535353535353535353535353535353535000000013636363636363636363636363636363636363636363636363636363636363636000000007f52f45d0185367e070815ade2",
    "3838a8b322a381d81102d343b7f0c9befb1429",
];

pub(super) struct Scenario {
    state: OrchestratorState,
    steps: Vec<(OrchestratorCommand, OrchestratorTransition)>,
    next_identity: u16,
}

impl Scenario {
    pub(super) fn new() -> Result<Self, &'static str> {
        let encoded = genesis_command_bytes()?;
        let command =
            decode_message::<OrchestratorCommandFrame>(encoded.as_slice(), CodecLimits::PRODUCTION)
                .map_err(|_| "decode pinned orchestrator genesis corpus")?
                .into_command();
        let transition = crate::start(&command).map_err(|_| "start orchestrator genesis")?;
        Ok(Self {
            state: transition.state().clone(),
            steps: vec![(command, transition)],
            next_identity: 1_000,
        })
    }

    pub(super) const fn state(&self) -> &OrchestratorState {
        &self.state
    }

    pub(super) fn next_event_id(&self) -> Result<EventId, &'static str> {
        EventId::new(bytes(self.next_identity.saturating_add(1)))
            .map_err(|_| "construct qualification event identity")
    }

    pub(super) fn apply(&mut self, kind: OrchestratorCommandKind) -> Result<(), &'static str> {
        let command_id = CommandId::new(bytes(self.next_identity))
            .map_err(|_| "construct qualification command identity")?;
        let event_id = self.next_event_id()?;
        self.next_identity =
            self.next_identity.checked_add(2).ok_or("qualification identity cursor overflowed")?;
        let command = OrchestratorCommand::new(
            command_id,
            event_id,
            self.state.binding().run_id(),
            self.state.sequence().get(),
            Some(self.state.last_event_id()),
            self.state.state_digest(),
            self.state.current_candidate().revision(),
            kind,
        )
        .map_err(|_| "construct qualification orchestrator command")?;
        let transition = crate::decide(&self.state, &command)
            .map_err(|_| "reduce qualification orchestrator command")?;
        self.state = transition.state().clone();
        self.steps.push((command, transition));
        Ok(())
    }

    pub(super) fn into_steps(self) -> Vec<(OrchestratorCommand, OrchestratorTransition)> {
        self.steps
    }
}

fn genesis_command_bytes() -> Result<Vec<u8>, &'static str> {
    let mut output = Vec::with_capacity(1_555);
    for chunk in GENESIS_COMMAND_HEX {
        let pairs = chunk.as_bytes().chunks_exact(2);
        if !pairs.remainder().is_empty() {
            return Err("pinned orchestrator genesis hex has an odd length");
        }
        for pair in pairs {
            let high = hex_digit(pair[0])?;
            let low = hex_digit(pair[1])?;
            output.push(high * 16 + low);
        }
    }
    if output.len() == 1_555 {
        Ok(output)
    } else {
        Err("pinned orchestrator genesis byte length differs")
    }
}

const fn hex_digit(value: u8) -> Result<u8, &'static str> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err("pinned orchestrator genesis hex has a non-hex digit"),
    }
}

pub(super) const fn bytes(value: u16) -> [u8; 16] {
    let [high, low] = value.to_be_bytes();
    let mut output = [1; 16];
    output[0] = high;
    output[1] = low;
    output
}
