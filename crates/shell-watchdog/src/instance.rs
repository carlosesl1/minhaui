#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstanceProbe {
    Unowned,
    OwnedByLiveProcess,
    OwnedByStaleProcess,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstanceDecision {
    RunPrimary,
    SignalExisting,
    ReplaceStaleOwner,
}

#[must_use]
pub const fn classify_instance(probe: InstanceProbe) -> InstanceDecision {
    match probe {
        InstanceProbe::Unowned => InstanceDecision::RunPrimary,
        InstanceProbe::OwnedByLiveProcess => InstanceDecision::SignalExisting,
        InstanceProbe::OwnedByStaleProcess => InstanceDecision::ReplaceStaleOwner,
    }
}
