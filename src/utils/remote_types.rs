use bincode::Encode;
// we use bincode serialization
#[derive(Encode)]
pub struct BinLifecycle {
    pub id: u32,
    pub ecu: u32, // Vec<u8>, // todo bincode-typescript doesn't support [u8;4] ... add support or misuse as u32
    pub nr_msgs: u32,
    pub start_time: u64,
    pub end_time: u64,
}

#[derive(Encode)]
pub struct BinDltMsg {
    pub index: u32, // todo use DltMessageIndexType!
    pub reception_time: u64,
    pub timestamp_dms: u32,
    pub ecu: u32,
    pub apid: u32,
    pub ctid: u32,
    pub lifecycle_id: u32, // todo use lifecycle::LifecycleId
    pub htyp: u8,
    pub mcnt: u8,
    pub verb_mstp_mtin: u8,
    pub noar: u8,
    pub payload_as_text: String, // todo and option for payload as vec[u8]?
}

#[derive(Encode)]
pub struct BinFileInfo {
    pub nr_msgs: u32, // todo change with index
}

#[derive(Encode)]
pub enum BinType {
    FileInfo(BinFileInfo),
    Lifecycles(Vec<BinLifecycle>),
    DltMsgs((u32, Vec<BinDltMsg>)), // stream id and Vec
}