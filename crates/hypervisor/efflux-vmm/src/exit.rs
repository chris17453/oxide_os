//! VM Exit Handling

use crate::memory::Gpa;

/// VM exit information
#[derive(Debug, Clone)]
pub struct VmExit {
    /// Exit reason
    pub reason: ExitReason,
    /// Additional data
    pub data: ExitData,
}

impl VmExit {
    pub fn new(reason: ExitReason) -> Self {
        VmExit {
            reason,
            data: ExitData::None,
        }
    }

    pub fn with_data(reason: ExitReason, data: ExitData) -> Self {
        VmExit { reason, data }
    }
}

/// VM exit reason
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitReason {
    /// Exception or NMI
    Exception,
    /// External interrupt
    ExternalInterrupt,
    /// Triple fault
    TripleFault,
    /// INIT signal
    Init,
    /// Startup IPI
    Sipi,
    /// I/O SMI
    IoSmi,
    /// Other SMI
    OtherSmi,
    /// Interrupt window
    InterruptWindow,
    /// NMI window
    NmiWindow,
    /// Task switch
    TaskSwitch,
    /// CPUID instruction
    Cpuid,
    /// GETSEC instruction
    Getsec,
    /// HLT instruction
    Hlt,
    /// INVD instruction
    Invd,
    /// INVLPG instruction
    Invlpg,
    /// RDPMC instruction
    Rdpmc,
    /// RDTSC instruction
    Rdtsc,
    /// RSM instruction
    Rsm,
    /// VMCALL instruction
    Vmcall,
    /// VMCLEAR instruction
    Vmclear,
    /// VMLAUNCH instruction
    Vmlaunch,
    /// VMPTRLD instruction
    Vmptrld,
    /// VMPTRST instruction
    Vmptrst,
    /// VMREAD instruction
    Vmread,
    /// VMRESUME instruction
    Vmresume,
    /// VMWRITE instruction
    Vmwrite,
    /// VMXOFF instruction
    Vmxoff,
    /// VMXON instruction
    Vmxon,
    /// Control register access
    CrAccess,
    /// MOV DR
    DrAccess,
    /// I/O instruction
    IoInstruction,
    /// RDMSR instruction
    Rdmsr,
    /// WRMSR instruction
    Wrmsr,
    /// VM-entry failure (invalid guest state)
    EntryFailGuestState,
    /// VM-entry failure (MSR loading)
    EntryFailMsr,
    /// MWAIT instruction
    Mwait,
    /// Monitor trap flag
    MonitorTrapFlag,
    /// MONITOR instruction
    Monitor,
    /// PAUSE instruction
    Pause,
    /// VM-entry failure (machine check)
    EntryFailMachineCheck,
    /// TPR below threshold
    TprBelowThreshold,
    /// APIC access
    ApicAccess,
    /// Virtualized EOI
    VirtualizedEoi,
    /// GDTR/IDTR access
    GdtrIdtrAccess,
    /// LDTR/TR access
    LdtrTrAccess,
    /// EPT violation
    EptViolation,
    /// EPT misconfiguration
    EptMisconfiguration,
    /// INVEPT instruction
    Invept,
    /// RDTSCP instruction
    Rdtscp,
    /// VMX preemption timer expired
    PreemptionTimer,
    /// INVVPID instruction
    Invvpid,
    /// WBINVD instruction
    Wbinvd,
    /// XSETBV instruction
    Xsetbv,
    /// APIC write
    ApicWrite,
    /// RDRAND instruction
    Rdrand,
    /// INVPCID instruction
    Invpcid,
    /// VMFUNC instruction
    Vmfunc,
    /// ENCLS instruction
    Encls,
    /// RDSEED instruction
    Rdseed,
    /// Page modification log full
    PmlFull,
    /// XSAVES instruction
    Xsaves,
    /// XRSTORS instruction
    Xrstors,
    /// SPP-related event
    Spp,
    /// UMWAIT instruction
    Umwait,
    /// TPAUSE instruction
    Tpause,
    /// Shutdown
    Shutdown,
    /// Unknown exit reason
    Unknown(u32),
}

impl From<u32> for ExitReason {
    fn from(value: u32) -> Self {
        match value {
            0 => ExitReason::Exception,
            1 => ExitReason::ExternalInterrupt,
            2 => ExitReason::TripleFault,
            3 => ExitReason::Init,
            4 => ExitReason::Sipi,
            5 => ExitReason::IoSmi,
            6 => ExitReason::OtherSmi,
            7 => ExitReason::InterruptWindow,
            8 => ExitReason::NmiWindow,
            9 => ExitReason::TaskSwitch,
            10 => ExitReason::Cpuid,
            11 => ExitReason::Getsec,
            12 => ExitReason::Hlt,
            13 => ExitReason::Invd,
            14 => ExitReason::Invlpg,
            15 => ExitReason::Rdpmc,
            16 => ExitReason::Rdtsc,
            17 => ExitReason::Rsm,
            18 => ExitReason::Vmcall,
            19 => ExitReason::Vmclear,
            20 => ExitReason::Vmlaunch,
            21 => ExitReason::Vmptrld,
            22 => ExitReason::Vmptrst,
            23 => ExitReason::Vmread,
            24 => ExitReason::Vmresume,
            25 => ExitReason::Vmwrite,
            26 => ExitReason::Vmxoff,
            27 => ExitReason::Vmxon,
            28 => ExitReason::CrAccess,
            29 => ExitReason::DrAccess,
            30 => ExitReason::IoInstruction,
            31 => ExitReason::Rdmsr,
            32 => ExitReason::Wrmsr,
            33 => ExitReason::EntryFailGuestState,
            34 => ExitReason::EntryFailMsr,
            36 => ExitReason::Mwait,
            37 => ExitReason::MonitorTrapFlag,
            39 => ExitReason::Monitor,
            40 => ExitReason::Pause,
            41 => ExitReason::EntryFailMachineCheck,
            43 => ExitReason::TprBelowThreshold,
            44 => ExitReason::ApicAccess,
            45 => ExitReason::VirtualizedEoi,
            46 => ExitReason::GdtrIdtrAccess,
            47 => ExitReason::LdtrTrAccess,
            48 => ExitReason::EptViolation,
            49 => ExitReason::EptMisconfiguration,
            50 => ExitReason::Invept,
            51 => ExitReason::Rdtscp,
            52 => ExitReason::PreemptionTimer,
            53 => ExitReason::Invvpid,
            54 => ExitReason::Wbinvd,
            55 => ExitReason::Xsetbv,
            56 => ExitReason::ApicWrite,
            57 => ExitReason::Rdrand,
            58 => ExitReason::Invpcid,
            59 => ExitReason::Vmfunc,
            60 => ExitReason::Encls,
            61 => ExitReason::Rdseed,
            62 => ExitReason::PmlFull,
            63 => ExitReason::Xsaves,
            64 => ExitReason::Xrstors,
            66 => ExitReason::Spp,
            67 => ExitReason::Umwait,
            68 => ExitReason::Tpause,
            n => ExitReason::Unknown(n),
        }
    }
}

/// Additional exit data
#[derive(Debug, Clone)]
pub enum ExitData {
    /// No additional data
    None,
    /// Exception info
    Exception(ExceptionInfo),
    /// I/O instruction info
    Io(IoInfo),
    /// EPT violation info
    EptViolation(EptViolationInfo),
    /// CR access info
    CrAccess(CrAccessInfo),
    /// MSR access info
    MsrAccess(MsrAccessInfo),
    /// CPUID info
    Cpuid(CpuidInfo),
    /// Hypercall info
    Hypercall(HypercallInfo),
}

/// Exception information
#[derive(Debug, Clone)]
pub struct ExceptionInfo {
    /// Exception vector
    pub vector: u8,
    /// Error code (if applicable)
    pub error_code: Option<u32>,
    /// CR2 (for page faults)
    pub cr2: Option<u64>,
}

/// I/O instruction information
#[derive(Debug, Clone)]
pub struct IoInfo {
    /// Port number
    pub port: u16,
    /// Size (1, 2, or 4 bytes)
    pub size: u8,
    /// Is write (true) or read (false)
    pub is_write: bool,
    /// Is string operation
    pub is_string: bool,
    /// Is REP prefix
    pub is_rep: bool,
    /// Data (for writes)
    pub data: u32,
}

/// EPT violation information
#[derive(Debug, Clone)]
pub struct EptViolationInfo {
    /// Guest physical address
    pub gpa: Gpa,
    /// Guest linear address (if valid)
    pub gla: Option<u64>,
    /// Was read access
    pub read: bool,
    /// Was write access
    pub write: bool,
    /// Was instruction fetch
    pub execute: bool,
    /// Is valid guest linear address
    pub gla_valid: bool,
}

/// Control register access information
#[derive(Debug, Clone)]
pub struct CrAccessInfo {
    /// CR number (0, 3, 4, 8)
    pub cr_num: u8,
    /// Access type (0=MOV to CR, 1=MOV from CR, 2=CLTS, 3=LMSW)
    pub access_type: u8,
    /// Register used
    pub reg: u8,
    /// Value (for writes)
    pub value: u64,
}

/// MSR access information
#[derive(Debug, Clone)]
pub struct MsrAccessInfo {
    /// MSR index
    pub index: u32,
    /// Is write
    pub is_write: bool,
    /// Value (for writes)
    pub value: u64,
}

/// CPUID information
#[derive(Debug, Clone)]
pub struct CpuidInfo {
    /// Leaf (EAX input)
    pub leaf: u32,
    /// Subleaf (ECX input)
    pub subleaf: u32,
}

/// Hypercall information
#[derive(Debug, Clone)]
pub struct HypercallInfo {
    /// Hypercall number
    pub nr: u64,
    /// Arguments
    pub args: [u64; 6],
}
