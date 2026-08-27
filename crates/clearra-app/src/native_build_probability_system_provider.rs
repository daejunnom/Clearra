//! Process-local admission provider for the durable native Build runtime.
//!
//! The provider proves that the complete requested worker set can be created
//! with the governed stack size while separate, simultaneously-live backing
//! allocations cover the channel/control, batch-owner, and result-owner
//! categories. It never accepts command- or environment-supplied byte claims.

use std::{
    hint::black_box,
    sync::{
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
        Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
};

use super::durable::{
    NativeBuildProbabilityAdmissionProvider, NativeBuildProbabilityAdmissionRequest,
    NativeBuildProbabilityHostProviderError, NativeBuildProbabilityProviderMeasurement,
};

const PROBE_PAGE_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemNativeBuildProbabilityAdmissionProvider;

impl NativeBuildProbabilityAdmissionProvider for SystemNativeBuildProbabilityAdmissionProvider {
    fn admit_native_build_probability(
        &self,
        request: NativeBuildProbabilityAdmissionRequest,
    ) -> Result<NativeBuildProbabilityProviderMeasurement, NativeBuildProbabilityHostProviderError>
    {
        SystemAdmissionProbe::run(request)
    }
}

struct SystemAdmissionProbe {
    channel_charge: TouchedAllocation,
    batch_owner_charge: TouchedAllocation,
    result_owner_charge: TouchedAllocation,
    _channels: ChannelProbe,
    worker_stack_bytes: u128,
}

impl SystemAdmissionProbe {
    fn run(
        request: NativeBuildProbabilityAdmissionRequest,
    ) -> Result<NativeBuildProbabilityProviderMeasurement, NativeBuildProbabilityHostProviderError>
    {
        if request.worker_count() == 0
            || request.maximum_task_count() == 0
            || request.candidate_batch_capacity() == 0
        {
            return Err(provider_error(
                "native_build_probability_system_admission_request_invalid",
            ));
        }

        // Keep all four categories alive together. The explicit charge
        // allocations make opaque standard-library channel bookkeeping an
        // over-accounted category rather than an unmeasured assumption.
        let channel_charge = TouchedAllocation::new(
            request.minimum_channel_control_bytes(),
            "native_build_probability_system_channel_allocation_unavailable",
        )?;
        let batch_owner_charge = TouchedAllocation::new(
            request.minimum_batch_owner_peak_bytes(),
            "native_build_probability_system_batch_owner_allocation_unavailable",
        )?;
        let result_owner_charge = TouchedAllocation::new(
            request.minimum_result_owner_peak_bytes(),
            "native_build_probability_system_result_owner_allocation_unavailable",
        )?;
        let channels = ChannelProbe::new(request)?;
        let worker_stack_bytes = probe_all_worker_stacks(request)?;
        let probe = Self {
            channel_charge,
            batch_owner_charge,
            result_owner_charge,
            _channels: channels,
            worker_stack_bytes,
        };

        black_box(&probe);
        NativeBuildProbabilityProviderMeasurement::new(
            probe.worker_stack_bytes,
            probe.channel_charge.actual_bytes(),
            probe.batch_owner_charge.actual_bytes(),
            probe.result_owner_charge.actual_bytes(),
        )
        .ok_or_else(|| provider_error("native_build_probability_system_measurement_unavailable"))
    }
}

struct TouchedAllocation {
    bytes: Vec<u8>,
}

impl TouchedAllocation {
    fn new(
        minimum_bytes: u128,
        unavailable_component: &'static str,
    ) -> Result<Self, NativeBuildProbabilityHostProviderError> {
        let minimum_bytes =
            usize::try_from(minimum_bytes).map_err(|_| provider_error(unavailable_component))?;
        if minimum_bytes == 0 {
            return Err(provider_error(unavailable_component));
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(minimum_bytes)
            .map_err(|_| provider_error(unavailable_component))?;
        bytes.resize(minimum_bytes, 0);
        for index in (0..minimum_bytes).step_by(PROBE_PAGE_BYTES) {
            bytes[index] = (index as u8).wrapping_add(1);
        }
        if let Some(last) = bytes.last_mut() {
            *last = last.wrapping_add(1);
        }
        black_box(&bytes);
        Ok(Self { bytes })
    }

    fn actual_bytes(&self) -> u128 {
        self.bytes.capacity() as u128
    }
}

struct ChannelProbe {
    _request_senders: Vec<SyncSender<u8>>,
    _request_receivers: Vec<Receiver<u8>>,
    _completion_sender: SyncSender<u8>,
    _completion_receiver: Receiver<u8>,
}

impl ChannelProbe {
    fn new(
        request: NativeBuildProbabilityAdmissionRequest,
    ) -> Result<Self, NativeBuildProbabilityHostProviderError> {
        let mut request_senders = Vec::new();
        let mut request_receivers = Vec::new();
        request_senders
            .try_reserve_exact(request.worker_count())
            .map_err(|_| provider_error("native_build_probability_system_channel_unavailable"))?;
        request_receivers
            .try_reserve_exact(request.worker_count())
            .map_err(|_| provider_error("native_build_probability_system_channel_unavailable"))?;

        for _ in 0..request.worker_count() {
            let (sender, receiver) = mpsc::sync_channel::<u8>(request.request_channel_capacity());
            exercise_buffered_channel(&sender, &receiver, request.request_channel_capacity())?;
            request_senders.push(sender);
            request_receivers.push(receiver);
        }
        let (completion_sender, completion_receiver) =
            mpsc::sync_channel::<u8>(request.completion_channel_capacity());
        exercise_buffered_channel(
            &completion_sender,
            &completion_receiver,
            request.completion_channel_capacity(),
        )?;

        Ok(Self {
            _request_senders: request_senders,
            _request_receivers: request_receivers,
            _completion_sender: completion_sender,
            _completion_receiver: completion_receiver,
        })
    }
}

fn exercise_buffered_channel(
    sender: &SyncSender<u8>,
    receiver: &Receiver<u8>,
    capacity: usize,
) -> Result<(), NativeBuildProbabilityHostProviderError> {
    if capacity == 0 {
        return Ok(());
    }
    sender.try_send(1).map_err(|error| match error {
        TrySendError::Full(_) | TrySendError::Disconnected(_) => {
            provider_error("native_build_probability_system_channel_unavailable")
        }
    })?;
    receiver.try_recv().map_err(|error| match error {
        TryRecvError::Empty | TryRecvError::Disconnected => {
            provider_error("native_build_probability_system_channel_unavailable")
        }
    })?;
    Ok(())
}

fn probe_all_worker_stacks(
    request: NativeBuildProbabilityAdmissionRequest,
) -> Result<u128, NativeBuildProbabilityHostProviderError> {
    let worker_count = request.worker_count();
    let worker_count_u128 = worker_count as u128;
    if worker_count == 0 || request.worker_stack_bytes() % worker_count_u128 != 0 {
        return Err(provider_error(
            "native_build_probability_system_worker_stack_request_invalid",
        ));
    }
    let stack_bytes =
        usize::try_from(request.worker_stack_bytes() / worker_count_u128).map_err(|_| {
            provider_error("native_build_probability_system_worker_stack_request_invalid")
        })?;
    if stack_bytes == 0 {
        return Err(provider_error(
            "native_build_probability_system_worker_stack_request_invalid",
        ));
    }

    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let (started_sender, started_receiver) = mpsc::sync_channel::<usize>(worker_count);
    let mut handles = Vec::<JoinHandle<()>>::new();
    handles
        .try_reserve_exact(worker_count)
        .map_err(|_| provider_error("native_build_probability_system_worker_stack_unavailable"))?;

    for worker_index in 0..worker_count {
        let worker_release = Arc::clone(&release);
        let started_sender = started_sender.clone();
        let spawn = thread::Builder::new()
            .name(format!("clearra-native-build-admission-{worker_index}"))
            .stack_size(stack_bytes)
            .spawn(move || {
                if started_sender.send(worker_index).is_err() {
                    return;
                }
                let (lock, ready) = &*worker_release;
                let guard = match lock.lock() {
                    Ok(guard) => guard,
                    Err(_) => return,
                };
                drop(ready.wait_while(guard, |released| !*released));
            });
        match spawn {
            Ok(handle) => handles.push(handle),
            Err(_) => {
                release_workers(&release);
                join_probe_workers(handles);
                return Err(provider_error(
                    "native_build_probability_system_worker_stack_unavailable",
                ));
            }
        }
    }
    drop(started_sender);

    for _ in 0..worker_count {
        if started_receiver.recv().is_err() {
            release_workers(&release);
            join_probe_workers(handles);
            return Err(provider_error(
                "native_build_probability_system_worker_stack_unavailable",
            ));
        }
    }
    release_workers(&release);
    if !join_probe_workers(handles) {
        return Err(provider_error(
            "native_build_probability_system_worker_stack_unavailable",
        ));
    }

    (stack_bytes as u128)
        .checked_mul(worker_count_u128)
        .ok_or_else(|| {
            provider_error("native_build_probability_system_worker_stack_request_invalid")
        })
}

fn release_workers(release: &Arc<(Mutex<bool>, Condvar)>) {
    let (lock, ready) = &**release;
    if let Ok(mut released) = lock.lock() {
        *released = true;
        ready.notify_all();
    }
}

fn join_probe_workers(handles: Vec<JoinHandle<()>>) -> bool {
    let mut all_joined = true;
    for handle in handles {
        if handle.join().is_err() {
            all_joined = false;
        }
    }
    all_joined
}

pub(crate) fn system_boot_uuid() -> Result<String, ()> {
    let mut uuid = [0_u8; 16];
    getrandom::fill(&mut uuid).map_err(|_| ())?;
    uuid[6] = (uuid[6] & 0x0f) | 0x40;
    uuid[8] = (uuid[8] & 0x3f) | 0x80;
    Ok(format!(
        "{}-{}-{}-{}-{}",
        hex(&uuid[..4]),
        hex(&uuid[4..6]),
        hex(&uuid[6..8]),
        hex(&uuid[8..10]),
        hex(&uuid[10..16]),
    ))
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

const fn provider_error(component: &'static str) -> NativeBuildProbabilityHostProviderError {
    NativeBuildProbabilityHostProviderError::new(component)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_boot_identity_is_canonical_random_v4() {
        let first = system_boot_uuid().expect("first system boot UUID");
        let second = system_boot_uuid().expect("second system boot UUID");

        assert_ne!(first, second);
        for value in [first, second] {
            assert_eq!(value.len(), 36);
            assert_eq!(value.as_bytes()[14], b'4');
            assert!(matches!(value.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
            assert!(value.bytes().enumerate().all(|(index, byte)| match index {
                8 | 13 | 18 | 23 => byte == b'-',
                _ => byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte),
            }));
        }
    }

    #[test]
    fn touched_allocation_reports_allocator_capacity_not_requested_scalar() {
        let allocation = TouchedAllocation::new(4097, "test_allocation_unavailable")
            .expect("allocation-backed charge");
        assert!(allocation.actual_bytes() >= 4097);
        assert_eq!(allocation.bytes.len(), 4097);
        assert_ne!(allocation.bytes[0], 0);
        assert_ne!(allocation.bytes[4096], 0);
    }
}
