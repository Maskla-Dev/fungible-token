use ft_io::*;
use gmeta::Metadata;
use gstd::{
    debug, errors::Result as GstdResult, exec, msg, ops::Range, prelude::*, ActorId, MessageId,
};
use hashbrown::HashMap;
use parity_scale_codec::DecodeAll;

const ZERO_ID: ActorId = ActorId::new([0u8; 32]);

const MAX_LEN: u64 = 5_000;

#[derive(Debug, Clone, Default)]
struct FungibleToken {
    /// Name of the token.
    name: String,
    /// Symbol of the token.
    symbol: String,
    /// Total supply of the token.
    total_supply: u128,
    /// Map to hold balances of token holders.
    balances: HashMap<ActorId, u128>,
    /// Map to hold allowance information of token holders.
    allowances: HashMap<ActorId, HashMap<ActorId, u128>>,
    /// Token's decimals.
    pub decimals: u8,
    /// State of Migration: None, on Receive, on Send
    pub migration_state: Option<MigrationState>,
}

static mut FUNGIBLE_TOKEN: Option<FungibleToken> = None;

impl FungibleToken {
    /// Executed on receiving `fungible-token-messages::MintInput`.
    fn mint(&mut self, amount: u128) {
        self.balances
            .entry(msg::source())
            .and_modify(|balance| *balance += amount)
            .or_insert(amount);
        self.total_supply += amount;
        msg::reply(
            FTEvent::Transfer {
                from: ZERO_ID,
                to: msg::source(),
                amount,
            },
            0,
        )
        .unwrap();
    }
    /// Executed on receiving `fungible-token-messages::BurnInput`.
    fn burn(&mut self, amount: u128) {
        if self.balances.get(&msg::source()).unwrap_or(&0) < &amount {
            panic!("Amount exceeds account balance");
        }
        self.balances
            .entry(msg::source())
            .and_modify(|balance| *balance -= amount);
        self.total_supply -= amount;

        msg::reply(
            FTEvent::Transfer {
                from: msg::source(),
                to: ZERO_ID,
                amount,
            },
            0,
        )
        .unwrap();
    }
    /// Executed on receiving `fungible-token-messages::TransferInput` or `fungible-token-messages::TransferFromInput`.
    /// Transfers `amount` tokens from `sender` account to `recipient` account.
    fn transfer(&mut self, from: &ActorId, to: &ActorId, amount: u128) {
        if from == &ZERO_ID || to == &ZERO_ID {
            panic!("Zero addresses");
        };
        if !self.can_transfer(from, amount) {
            panic!("Not allowed to transfer")
        }
        if self.balances.get(from).unwrap_or(&0) < &amount {
            panic!("Amount exceeds account balance");
        }
        self.balances
            .entry(*from)
            .and_modify(|balance| *balance -= amount);
        self.balances
            .entry(*to)
            .and_modify(|balance| *balance += amount)
            .or_insert(amount);
        msg::reply(
            FTEvent::Transfer {
                from: *from,
                to: *to,
                amount,
            },
            0,
        )
        .unwrap();
    }

    /// Executed on receiving `fungible-token-messages::ApproveInput`.
    fn approve(&mut self, to: &ActorId, amount: u128) {
        if to == &ZERO_ID {
            panic!("Approve to zero address");
        }
        self.allowances
            .entry(msg::source())
            .or_default()
            .insert(*to, amount);
        msg::reply(
            FTEvent::Approve {
                from: msg::source(),
                to: *to,
                amount,
            },
            0,
        )
        .unwrap();
    }

    fn can_transfer(&mut self, from: &ActorId, amount: u128) -> bool {
        if from == &msg::source()
            || from == &exec::origin()
            || self.balances.get(&msg::source()).unwrap_or(&0) >= &amount
        {
            return true;
        }
        if let Some(allowed_amount) = self
            .allowances
            .get(from)
            .and_then(|m| m.get(&msg::source()))
        {
            if allowed_amount >= &amount {
                self.allowances.entry(*from).and_modify(|m| {
                    m.entry(msg::source()).and_modify(|a| *a -= amount);
                });
                return true;
            }
        }
        false
    }

    fn prepare_new_state(new_state: IoFungibleToken) -> Self {
        let IoFungibleToken {
            name,
            symbol,
            total_supply,
            balances,
            allowances,
            decimals,
            migration_state,
        } = new_state;

        let allowances = allowances
            .into_iter()
            .map(|(actor_id, allows)| {
                let allows = allows.into_iter().collect();
                (actor_id, allows)
            })
            .collect();

        Self {
            name,
            symbol,
            total_supply,
            balances: balances.into_iter().collect(),
            allowances,
            decimals,
            migration_state,
        }
    }

    fn set_migration_state(&mut self, migration_state: MigrationState) {
        gstd::debug!(
            "AZOYAN set_migration_state() ProgramID: {}, {}",
            self.name,
            migration_state
        );
        self.migration_state = Some(migration_state);
    }

    /// Executed on receiving `fungible-token-messages::MigrateState`.
    fn begin_migration(&mut self, program_id: ActorId) {
        let ft_io = self.to_io_cloned();
        let data = ft_io.encode();
        let total_len = data.len() as u64;

        let total_checksum = calc_checksum(&data);

        let role = MigrationRole::Sender {
            migrate_to: program_id,
        };

        let mut chunk_size = total_len;

        if total_len > MAX_LEN {
            chunk_size = MAX_LEN;
        };

        let ranges = split_into_ranges(&data, chunk_size);

        let first_range = ranges[0].clone();
        let first_data = data[first_range.start as usize..first_range.end as usize].to_vec();

        let part = Part {
            current: 0,
            total: ranges.len() as u64,
        };
        let ms = MigrationState {
            migration_role: role,
            data,
            part,
            ranges,
            total_checksum,
            message_id: MessageId::zero(),
        };

        // Seal our state
        self.set_migration_state(ms);
        // Now we are in MigrationState

        let upgrade_state = FTAction::UpgradeState {
            data: first_data,
            total_checksum,
            range: first_range,
            part,
        };

        gstd::debug!(
            "AZOYAN begin_migration() ProgramID: {:?}, part: {:?}, is_migration_state = {}",
            self.name,
            part,
            self.migration_state.is_some()
        );

        let message_id = msg::send(program_id, upgrade_state, 0).expect("Error at sending upgrade");

        self.migration_state.as_mut().unwrap().message_id = message_id;
    }

    fn continue_migration(&mut self, program_id: ActorId) {
        let migration_state = self.migration_state.as_mut().unwrap();

        migration_state.part.current += 1;
        let range = &migration_state.ranges[migration_state.part.current as usize];


        let data = migration_state.data[range.start as usize..range.end as usize]
            .iter()
            .copied()
            .collect();
        let part = migration_state.part;
        let upgrade_state = FTAction::UpgradeState {
            data,
            total_checksum: migration_state.total_checksum,
            part,
            range: range.clone(),
        };

        gstd::debug!(
            "AZOYAN continue_migration() ProgramID: {:?}, part: {:?}, is_migration_state = {}",
            self.name,
            part,
            self.migration_state.is_some()
        );

        msg::send(program_id, upgrade_state, 0).expect("Error at sending upgrade");
    }

    fn begin_upgrade(
        &mut self,
        data: Vec<u8>,
        total_checksum: Checksum,
        part: Part,
        range: Range<u64>,
    ) {
        gstd::debug!(
            "AZOYAN begin_upgrade(): ProgramID: {:?}, Part: {:?}, range: {:?}",
            self.name,
            part,
            range
        );
        let checksum = calc_checksum(&data);
        if total_checksum != checksum {
            let migration_state = MigrationState {
                migration_role: MigrationRole::Receiver {
                    migration_initiator: msg::source(),
                },
                ranges: vec![range],
                part,
                total_checksum,
                data,
                message_id: msg::id(),
            };
            self.set_migration_state(migration_state);
        }
        msg::reply(FTEvent::StateUpgraded { checksum }, 0).expect("Can't send reply");
    }

    fn continue_upgrade(
        &mut self,
        data: Vec<u8>,
        total_checksum: Checksum,
        part: Part,
        range: Range<u64>,
    ) {
        let migration_state = self.migration_state.as_mut().unwrap();
        match migration_state.migration_role {
            MigrationRole::Sender { .. } => {
                msg::reply(FTEvent::AlreadyInMigrationState, 0).expect("Can't reply");
            }
            MigrationRole::Receiver {
                migration_initiator,
            } => {
                // We should check that this message received from Migration Initiator
                // Don't handle another UpgradeState actions
                if msg::source() != migration_initiator {
                    msg::reply(FTEvent::AlreadyInMigrationState, 0).expect("Can't reply");
                    return;
                }
                gstd::debug!(
                    "AZOYAN continue_upgrade(): ProgramID: {:?}, Part: {:?}, range: {:?}",
                    self.name,
                    part,
                    range
                );
                migration_state.data.extend(&data);
                migration_state.part = part;
                migration_state.ranges.push(range);

                let checksum = calc_checksum(&migration_state.data);
                if total_checksum == checksum {
                    let new_state =
                        IoFungibleToken::decode_all(&mut migration_state.data.as_slice())
                            .expect("Decode IoFungibleToken");
                    let new_ft = Self::prepare_new_state(new_state);
                    *self = new_ft;                  
                }
                msg::reply(FTEvent::StateUpgraded { checksum }, 0).expect("Can't reply");
            }
        }
    }

    fn to_io_cloned(&self) -> <FungibleTokenMetadata as Metadata>::State {
        let FungibleToken {
            name,
            symbol,
            total_supply,
            balances,
            allowances,
            decimals,
            migration_state,
        } = self.clone();

        let balances = balances.iter().map(|(k, v)| (*k, *v)).collect();
        let allowances = allowances
            .iter()
            .map(|(id, allowance)| (*id, allowance.iter().map(|(k, v)| (*k, *v)).collect()))
            .collect();
        IoFungibleToken {
            name,
            symbol,
            total_supply,
            balances,
            allowances,
            decimals,
            migration_state,
        }
    }
}

fn common_state() -> <FungibleTokenMetadata as Metadata>::State {
    static_mut_state().to_io_cloned()
}

fn static_mut_state() -> &'static mut FungibleToken {
    unsafe { FUNGIBLE_TOKEN.get_or_insert(Default::default()) }
}

#[no_mangle]
extern "C" fn state() {
    reply(common_state())
        .expect("Failed to encode or reply with `<AppMetadata as Metadata>::State` from `state()`");
}

#[no_mangle]
extern "C" fn metahash() {
    let metahash: [u8; 32] = include!("../.metahash");
    reply(metahash).expect("Failed to encode or reply with `[u8; 32]` from `metahash()`");
}

fn reply(payload: impl Encode) -> GstdResult<MessageId> {
    msg::reply(payload, 0)
}

#[no_mangle]
extern "C" fn handle() {
    let action: FTAction = msg::load().expect("Could not load Action");
    let ft: &mut FungibleToken = unsafe { FUNGIBLE_TOKEN.get_or_insert(Default::default()) };

    // Check that contract not in Migrate or Upgrade State
    if let Some(_migration_state) = &ft.migration_state {
        match action {
            FTAction::Mint(amount) => unreachable!(),
            FTAction::Burn(amount) => unreachable!(),
            FTAction::Transfer { from, to, amount } => unreachable!(),
            FTAction::Approve { to, amount } => unreachable!(),
            FTAction::TotalSupply => unreachable!(),
            FTAction::BalanceOf(account) => unreachable!(),
            _ => {}
        }
    }

    match action {
        FTAction::Mint(amount) => {
            ft.mint(amount);
        }
        FTAction::Burn(amount) => {
            ft.burn(amount);
        }
        FTAction::Transfer { from, to, amount } => {
            ft.transfer(&from, &to, amount);
        }
        FTAction::Approve { to, amount } => {
            ft.approve(&to, amount);
        }
        FTAction::TotalSupply => {
            msg::reply(FTEvent::TotalSupply(ft.total_supply), 0).unwrap();
        }
        FTAction::BalanceOf(account) => {
            let balance = ft.balances.get(&account).unwrap_or(&0);
            msg::reply(FTEvent::Balance(*balance), 0).unwrap();
        }
        FTAction::UpgradeState {
            data,
            total_checksum,
            part,
            range,
        } => {
            if let Some(migration_state) = &ft.migration_state {
                gstd::debug!(
                    "AZOYAN After continue_upgrade() ProgramID: {}, migration_state = {}",
                    ft.name,
                    migration_state
                );
                ft.continue_upgrade(data, total_checksum, part, range);
            } else {
                // ft.begin_upgrade(data, total_checksum, part, range);


                gstd::debug!(
                    "AZOYAN begin_upgrade(): ProgramID: {:?}, Part: {:?}, range: {:?}",
                    ft.name,
                    part,
                    range
                );
                let checksum = calc_checksum(&data);
                if total_checksum != checksum {
                    let migration_state = MigrationState {
                        migration_role: MigrationRole::Receiver {
                            migration_initiator: msg::source(),
                        },
                        ranges: vec![range],
                        part,
                        total_checksum,
                        data,
                        message_id: msg::id(),
                    };
                    // self.set_migration_state(migration_state);

                    gstd::debug!(
                        "AZOYAN set_migration_state() ProgramID: {}, {}",
                        ft.name,
                        migration_state
                    );
                    ft.migration_state = Some(migration_state);
                }
                else {
                    let new_state =
                    IoFungibleToken::decode_all(&mut data.as_slice())
                    .expect("Decode IoFungibleToken");
                let new_ft = FungibleToken::prepare_new_state(new_state);
                gstd::debug!(
                    "AZOYAN Upgraded ProgramID: {}, new: {}",
                    ft.name,
                    new_ft.name
                );
                *ft = new_ft;
                }
                gstd::debug!(
                    "AZOYAN After begin_upgrade() ProgramID: {}, migration_state = None",
                    ft.name
                );
                msg::reply(FTEvent::StateUpgraded { checksum }, 0).expect("Can't send reply");
            }
        }
        FTAction::MigrateState(program_id) => {
            gstd::debug!(
                "AZOYAN Before MigrateState ProgramID: {}, is_migration_state = {}",
                ft.name,
                ft.migration_state.is_some()
            );
            if ft.migration_state.is_some() {
                ft.continue_migration(program_id);
            } else {
                ft.begin_migration(program_id);
            }
            gstd::debug!(
                "AZOYAN After MigrateState ProgramID: {}, is_migration_state = {}",
                ft.name,
                ft.migration_state.is_some()
            );
        }
    }
}

#[no_mangle]
extern "C" fn handle_reply() {
    let reply_to_msg_id = msg::reply_to().expect("Can't extract reply_to MessageId");

    let ft = static_mut_state();

    if let Some(migration_state) = &ft.migration_state {
        if migration_state.message_id != reply_to_msg_id {
            gstd::debug!(
                "ProgramID: {}, Incorrect Reply from ProgramID: {:?}, message id: {:?}, expected: {:?}",
                ft.name,
                msg::source(),
                reply_to_msg_id,
                migration_state.message_id
            );
            return;
        }

        match migration_state.migration_role {
            MigrationRole::Sender { migrate_to: _ } => {
                let reply: FTEvent = msg::load().expect("Failed to decode the reply");
                match reply {
                    FTEvent::StateUpgraded { checksum } => {
                        gstd::debug!(
                            "AZOYAN ProgramID: {:?}, StateUpgraded from ProgramID: {:?}, total_checksum: {}, checksum: {}",
                            ft.name,
                            msg::source(),
                            migration_state.total_checksum,
                            checksum
                        );

                        if migration_state.total_checksum == checksum {
                            ft.migration_state = None;
                            gstd::debug!("AZOYAN Migration done");
                        } else {
                            gstd::debug!("AZOYAN ProgramID: {:?} Ready ot continue. Send action MigrateState to continue",
                        gstd::exec::program_id());
                        }
                    }
                    FTEvent::AlreadyInMigrationState => {
                        gstd::debug!("AZOYAN Already in Migration State")
                    }
                    _ => unreachable!("Unexpected replies when contract at migration state"),
                }
            }
            MigrationRole::Receiver {
                migration_initiator: _,
            } => unreachable!("Only migration initiator handle replies"),
        }
    }
}

#[no_mangle]
extern "C" fn init() {
    let init: Initialize = msg::load().expect("Unable to decode InitConfig");

    let ft = match init {
        Initialize::Config(config) => FungibleToken {
            name: config.name,
            symbol: config.symbol,
            decimals: config.decimals,
            ..Default::default()
        },
        Initialize::State(io_ft) => FungibleToken::prepare_new_state(io_ft),
    };

    unsafe { FUNGIBLE_TOKEN = Some(ft) };
}

#[no_mangle]
extern "C" fn meta_state() -> *mut [i32; 2] {
    let query: State = msg::load().expect("failed to decode input argument");
    let ft: &mut FungibleToken = unsafe { FUNGIBLE_TOKEN.get_or_insert(Default::default()) };
    debug!("{:?}", query);
    let encoded = match query {
        State::Name => StateReply::Name(ft.name.clone()),
        State::Symbol => StateReply::Name(ft.symbol.clone()),
        State::Decimals => StateReply::Decimals(ft.decimals),
        State::TotalSupply => StateReply::TotalSupply(ft.total_supply),
        State::BalanceOf(account) => {
            let balance = ft.balances.get(&account).unwrap_or(&0);
            StateReply::Balance(*balance)
        }
    }
    .encode();
    gstd::util::to_leak_ptr(encoded)
}

#[derive(Debug, Encode, Decode, TypeInfo)]
#[codec(crate = gstd::codec)]
#[scale_info(crate = gstd::scale_info)]
pub enum State {
    Name,
    Symbol,
    Decimals,
    TotalSupply,
    BalanceOf(ActorId),
}

#[derive(Debug, Encode, Decode, TypeInfo)]
#[codec(crate = gstd::codec)]
#[scale_info(crate = gstd::scale_info)]
pub enum StateReply {
    Name(String),
    Symbol(String),
    Decimals(u8),
    TotalSupply(u128),
    Balance(u128),
}

fn calc_checksum(data: &[u8]) -> Checksum {
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(data);
    hasher.finalize()
}

fn split_into_ranges(vec: &[u8], chunk_size: u64) -> Vec<Range<u64>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut end = chunk_size;

    let vec_len = vec.len() as u64;

    while end < vec_len {
        ranges.push(start..end);
        start = end;
        end += chunk_size;
    }

    if start < vec_len {
        ranges.push(start..vec_len);
    }

    ranges
}
