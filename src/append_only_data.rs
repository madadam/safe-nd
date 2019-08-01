// Copyright 2019 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under the MIT license <LICENSE-MIT
// https://opensource.org/licenses/MIT> or the Modified BSD license <LICENSE-BSD
// https://opensource.org/licenses/BSD-3-Clause>, at your option. This file may not be copied,
// modified, or distributed except according to those terms. Please review the Licences for the
// specific language governing permissions and limitations relating to use of the SAFE Network
// Software.

use crate::{utils, Error, PublicKey, Result, Seq, Unseq, XorName};
use multibase::Decodable;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{self, Debug, Formatter},
    hash::Hash,
    ops::Range,
};

pub type PubSeqAppendOnlyData = AppendOnlyData<PubPermissions, Seq>;
pub type PubUnseqAppendOnlyData = AppendOnlyData<PubPermissions, Unseq>;
pub type UnpubSeqAppendOnlyData = AppendOnlyData<UnpubPermissions, Seq>;
pub type UnpubUnseqAppendOnlyData = AppendOnlyData<UnpubPermissions, Unseq>;
pub type Entries = Vec<Entry>;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Debug)]
pub enum User {
    Anyone,
    Key(PublicKey),
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Action {
    Read,
    Append,
    ManagePermissions,
}

#[derive(Copy, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Index {
    FromStart(u64), // Absolute index
    FromEnd(u64),   // Relative index - start counting from the end
}

impl From<u64> for Index {
    fn from(index: u64) -> Self {
        Index::FromStart(index)
    }
}

// Set of data, owners, permissions Indices.
#[derive(Copy, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Indices {
    entries_index: u64,
    owners_index: u64,
    permissions_index: u64,
}

impl Indices {
    pub fn new(entries_index: u64, owners_index: u64, permissions_index: u64) -> Self {
        Indices {
            entries_index,
            owners_index,
            permissions_index,
        }
    }

    pub fn entries_index(&self) -> u64 {
        self.entries_index
    }

    pub fn owners_index(&self) -> u64 {
        self.owners_index
    }

    pub fn permissions_index(&self) -> u64 {
        self.permissions_index
    }
}

#[derive(Copy, Clone, Serialize, Deserialize, PartialEq, PartialOrd, Ord, Eq, Hash, Debug)]
pub struct UnpubPermissionSet {
    read: bool,
    append: bool,
    manage_permissions: bool,
}

impl UnpubPermissionSet {
    pub fn new(read: bool, append: bool, manage_perms: bool) -> Self {
        UnpubPermissionSet {
            read,
            append,
            manage_permissions: manage_perms,
        }
    }

    pub fn set_perms(&mut self, read: bool, append: bool, manage_perms: bool) {
        self.read = read;
        self.append = append;
        self.manage_permissions = manage_perms;
    }

    pub fn is_allowed(self, action: Action) -> bool {
        match action {
            Action::Read => self.read,
            Action::Append => self.append,
            Action::ManagePermissions => self.manage_permissions,
        }
    }
}

#[derive(Copy, Clone, Serialize, Deserialize, PartialEq, PartialOrd, Ord, Eq, Hash, Debug)]
pub struct PubPermissionSet {
    append: Option<bool>,
    manage_permissions: Option<bool>,
}

impl PubPermissionSet {
    pub fn new(append: impl Into<Option<bool>>, manage_perms: impl Into<Option<bool>>) -> Self {
        PubPermissionSet {
            append: append.into(),
            manage_permissions: manage_perms.into(),
        }
    }

    pub fn set_perms(
        &mut self,
        append: impl Into<Option<bool>>,
        manage_perms: impl Into<Option<bool>>,
    ) {
        self.append = append.into();
        self.manage_permissions = manage_perms.into();
    }

    pub fn is_allowed(self, action: Action) -> Option<bool> {
        match action {
            Action::Read => Some(true), // It's published data, so it's always allowed to read it.
            Action::Append => self.append,
            Action::ManagePermissions => self.manage_permissions,
        }
    }
}

pub trait Perm: Clone + Eq + Ord + Hash + Serialize + DeserializeOwned {
    fn is_action_allowed(&self, requester: PublicKey, action: Action) -> Result<()>;
    fn entries_index(&self) -> u64;
    fn owners_index(&self) -> u64;
}

#[derive(Clone, Serialize, Deserialize, PartialEq, PartialOrd, Ord, Eq, Hash, Debug)]
pub struct UnpubPermissions {
    pub permissions: BTreeMap<PublicKey, UnpubPermissionSet>,
    /// The current index of the data when this permission change happened
    pub entries_index: u64,
    /// The current index of the owners when this permission change happened
    pub owners_index: u64,
}

impl UnpubPermissions {
    pub fn permissions(&self) -> &BTreeMap<PublicKey, UnpubPermissionSet> {
        &self.permissions
    }
}

impl Perm for UnpubPermissions {
    fn is_action_allowed(&self, requester: PublicKey, action: Action) -> Result<()> {
        match self.permissions.get(&requester) {
            Some(perms) => {
                if perms.is_allowed(action) {
                    Ok(())
                } else {
                    Err(Error::AccessDenied)
                }
            }
            None => Err(Error::InvalidPermissions),
        }
    }

    fn entries_index(&self) -> u64 {
        self.entries_index
    }

    fn owners_index(&self) -> u64 {
        self.owners_index
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, PartialOrd, Ord, Eq, Hash, Debug)]
pub struct PubPermissions {
    pub permissions: BTreeMap<User, PubPermissionSet>,
    /// The current index of the data when this permission change happened
    pub entries_index: u64,
    /// The current index of the owners when this permission change happened
    pub owners_index: u64,
}

impl PubPermissions {
    fn is_action_allowed_by_user(&self, user: &User, action: Action) -> Option<bool> {
        self.permissions
            .get(user)
            .and_then(|perms| perms.is_allowed(action))
    }

    pub fn permissions(&self) -> &BTreeMap<User, PubPermissionSet> {
        &self.permissions
    }
}

impl Perm for PubPermissions {
    fn is_action_allowed(&self, requester: PublicKey, action: Action) -> Result<()> {
        match self
            .is_action_allowed_by_user(&User::Key(requester), action)
            .or_else(|| self.is_action_allowed_by_user(&User::Anyone, action))
        {
            Some(true) => Ok(()),
            Some(false) => Err(Error::AccessDenied),
            None => Err(Error::InvalidPermissions),
        }
    }

    fn entries_index(&self) -> u64 {
        self.entries_index
    }

    fn owners_index(&self) -> u64 {
        self.owners_index
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, PartialOrd, Ord, Eq, Hash, Debug)]
pub enum Permissions {
    Pub(PubPermissions),
    Unpub(UnpubPermissions),
}

impl From<UnpubPermissions> for Permissions {
    fn from(permissions: UnpubPermissions) -> Self {
        Permissions::Unpub(permissions)
    }
}

impl From<PubPermissions> for Permissions {
    fn from(permissions: PubPermissions) -> Self {
        Permissions::Pub(permissions)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Debug)]
pub struct Owner {
    pub public_key: PublicKey,
    /// The current index of the data when this ownership change happened
    pub entries_index: u64,
    /// The current index of the permissions when this ownership change happened
    pub permissions_index: u64,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default, Debug)]
pub struct Entry {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

impl Entry {
    pub fn new(key: Vec<u8>, value: Vec<u8>) -> Self {
        Self { key, value }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub struct AppendOnlyData<P, S> {
    address: Address,
    data: Entries,
    permissions: Vec<P>,
    // This is the history of owners, with each entry representing an owner.  Each single owner
    // could represent an individual user, or a group of users, depending on the `PublicKey` type.
    owners: Vec<Owner>,
    _flavour: S,
}

/// Common methods for all `AppendOnlyData` flavours.
impl<P, S> AppendOnlyData<P, S>
where
    P: Perm,
    S: Copy,
{
    /// Returns the data shell - that is - everything except the entries themselves.
    pub fn shell(&self, entries_index: impl Into<Index>) -> Result<Self> {
        let entries_index = to_absolute_index(entries_index.into(), self.entries_index() as usize)
            .ok_or(Error::NoSuchEntry)? as u64;

        let permissions = self
            .permissions
            .iter()
            .filter(|perm| perm.entries_index() <= entries_index)
            .cloned()
            .collect();

        let owners = self
            .owners
            .iter()
            .filter(|owner| owner.entries_index <= entries_index)
            .cloned()
            .collect();

        Ok(Self {
            address: self.address,
            data: Vec::new(),
            permissions,
            owners,
            _flavour: self._flavour,
        })
    }

    /// Return a value for the given key (if it is present).
    pub fn get(&self, key: &[u8]) -> Option<&Vec<u8>> {
        self.data.iter().find_map(|entry| {
            if entry.key.as_slice() == key {
                Some(&entry.value)
            } else {
                None
            }
        })
    }

    /// Return the last entry in the Data (if it is present).
    pub fn last_entry(&self) -> Option<&Entry> {
        self.data.last()
    }

    /// Get a list of keys and values with the given indices.
    pub fn in_range(&self, start: Index, end: Index) -> Option<Entries> {
        let range = to_absolute_range(start, end, self.data.len())?;
        Some(self.data[range].to_vec())
    }

    /// Return all entries.
    pub fn entries(&self) -> &Entries {
        &self.data
    }

    /// Return the address of this AppendOnlyData.
    pub fn address(&self) -> &Address {
        &self.address
    }

    /// Return the name of this AppendOnlyData.
    pub fn name(&self) -> &XorName {
        self.address.name()
    }

    /// Return the type tag of this AppendOnlyData.
    pub fn tag(&self) -> u64 {
        self.address.tag()
    }

    /// Return the last entry index.
    pub fn entries_index(&self) -> u64 {
        self.data.len() as u64
    }

    /// Return the last owners index.
    pub fn owners_index(&self) -> u64 {
        self.owners.len() as u64
    }

    /// Return the last permissions index.
    pub fn permissions_index(&self) -> u64 {
        self.permissions.len() as u64
    }

    /// Get a complete list of permissions from the entry in the permissions list at the specified
    /// index.
    pub fn permissions_range(&self, start: Index, end: Index) -> Option<&[P]> {
        let range = to_absolute_range(start, end, self.permissions.len())?;
        Some(&self.permissions[range])
    }

    /// Add a new permissions entry.
    /// The `Perm` struct should contain valid indices.
    pub fn append_permissions(&mut self, permissions: P, permissions_idx: u64) -> Result<()> {
        if permissions.entries_index() != self.entries_index() {
            return Err(Error::InvalidSuccessor(self.entries_index()));
        }
        if permissions.owners_index() != self.owners_index() {
            return Err(Error::InvalidOwnersSuccessor(self.owners_index()));
        }
        if self.permissions_index() != permissions_idx {
            return Err(Error::InvalidSuccessor(self.permissions_index()));
        }
        self.permissions.push(permissions);
        Ok(())
    }

    /// Fetch perms at index.
    pub fn permissions(&self, index: impl Into<Index>) -> Option<&P> {
        let index = to_absolute_index(index.into(), self.permissions.len())?;
        self.permissions.get(index)
    }

    pub fn check_permission(&self, requester: PublicKey, action: Action) -> Result<()> {
        if self
            .owner(Index::FromEnd(1))
            .ok_or(Error::InvalidOwners)?
            .public_key
            == requester
        {
            Ok(())
        } else {
            self.permissions(Index::FromEnd(1))
                .ok_or(Error::InvalidPermissions)?
                .is_action_allowed(requester, action)
        }
    }

    /// Fetch owner at index.
    pub fn owner(&self, index: impl Into<Index>) -> Option<&Owner> {
        let index = to_absolute_index(index.into(), self.owners.len())?;
        self.owners.get(index)
    }

    /// Get a complete list of owners from the entry in the permissions list at the specified index.
    pub fn owners_range(&self, start: Index, end: Index) -> Option<&[Owner]> {
        let range = to_absolute_range(start, end, self.owners.len())?;
        Some(&self.owners[range])
    }
    /// Add a new owner entry.
    pub fn append_owner(&mut self, owner: Owner, index: u64) -> Result<()> {
        if owner.entries_index != self.entries_index() {
            return Err(Error::InvalidSuccessor(self.entries_index()));
        }
        if owner.permissions_index != self.permissions_index() {
            return Err(Error::InvalidPermissionsSuccessor(self.permissions_index()));
        }
        if self.owners_index() != index {
            return Err(Error::InvalidSuccessor(self.owners_index()));
        }
        self.owners.push(owner);
        Ok(())
    }

    /// Check if the requester is the last owner.
    pub fn check_is_last_owner(&self, requester: PublicKey) -> Result<()> {
        if self
            .owner(Index::FromEnd(1))
            .ok_or_else(|| Error::InvalidOwners)?
            .public_key
            == requester
        {
            Ok(())
        } else {
            Err(Error::AccessDenied)
        }
    }

    pub fn indices(&self) -> Indices {
        Indices::new(
            self.entries_index(),
            self.owners_index(),
            self.permissions_index(),
        )
    }
}

/// Common methods for unsequential flavours.
impl<P: Perm> AppendOnlyData<P, Unseq> {
    /// Append new entries.
    pub fn append(&mut self, mut entries: Entries) -> Result<()> {
        check_dup(&self.data, entries.as_mut())?;

        self.data.extend(entries);
        Ok(())
    }
}

/// Common methods for sequential flavours.
impl<P: Perm> AppendOnlyData<P, Seq> {
    /// Append new entries.
    ///
    /// If the specified `last_entries_index` does not match the last recorded entries index, an
    /// error will be returned.
    pub fn append(&mut self, mut entries: Entries, last_entries_index: u64) -> Result<()> {
        check_dup(&self.data, entries.as_mut())?;

        if last_entries_index != self.data.len() as u64 {
            return Err(Error::InvalidSuccessor(self.data.len() as u64));
        }

        self.data.extend(entries);
        Ok(())
    }
}

/// Published + Sequential
impl AppendOnlyData<PubPermissions, Seq> {
    pub fn new(name: XorName, tag: u64) -> Self {
        Self {
            address: Address::PubSeq { name, tag },
            data: Vec::new(),
            permissions: Vec::new(),
            owners: Vec::new(),
            _flavour: Seq,
        }
    }
}

impl Debug for AppendOnlyData<PubPermissions, Seq> {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(formatter, "PubSeqAppendOnlyData {:?}", self.name())
    }
}

/// Published + Unsequential
impl AppendOnlyData<PubPermissions, Unseq> {
    pub fn new(name: XorName, tag: u64) -> Self {
        Self {
            address: Address::PubUnseq { name, tag },
            data: Vec::new(),
            permissions: Vec::new(),
            owners: Vec::new(),
            _flavour: Unseq,
        }
    }
}

impl Debug for AppendOnlyData<PubPermissions, Unseq> {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(formatter, "PubUnseqAppendOnlyData {:?}", self.name())
    }
}

/// Unpublished + Sequential
impl AppendOnlyData<UnpubPermissions, Seq> {
    pub fn new(name: XorName, tag: u64) -> Self {
        Self {
            address: Address::UnpubSeq { name, tag },
            data: Vec::new(),
            permissions: Vec::new(),
            owners: Vec::new(),
            _flavour: Seq,
        }
    }
}

impl Debug for AppendOnlyData<UnpubPermissions, Seq> {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(formatter, "UnpubSeqAppendOnlyData {:?}", self.name())
    }
}

/// Unpublished + Unsequential
impl AppendOnlyData<UnpubPermissions, Unseq> {
    pub fn new(name: XorName, tag: u64) -> Self {
        Self {
            address: Address::UnpubUnseq { name, tag },
            data: Vec::new(),
            permissions: Vec::new(),
            owners: Vec::new(),
            _flavour: Unseq,
        }
    }
}

impl Debug for AppendOnlyData<UnpubPermissions, Unseq> {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(formatter, "UnpubUnseqAppendOnlyData {:?}", self.name())
    }
}

fn check_dup(data: &[Entry], entries: &mut Entries) -> Result<()> {
    let new: BTreeSet<&Vec<u8>> = entries.iter().map(|entry| &entry.key).collect();

    // If duplicate entries are present in the push.
    if new.len() < entries.len() {
        return Err(Error::DuplicateEntryKeys);
    }

    let existing: BTreeSet<&Vec<u8>> = data.iter().map(|entry| &entry.key).collect();
    if !existing.is_disjoint(&new) {
        let dup: Entries = entries
            .drain(..)
            .filter(|entry| existing.contains(&entry.key))
            .collect();
        return Err(Error::KeysExist(dup));
    }
    Ok(())
}

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, Debug)]
pub enum Kind {
    PubSeq,
    PubUnseq,
    UnpubSeq,
    UnpubUnseq,
}

impl Kind {
    pub fn is_pub(self) -> bool {
        self == Kind::PubSeq || self == Kind::PubUnseq
    }

    pub fn is_unpub(self) -> bool {
        !self.is_pub()
    }

    pub fn is_seq(self) -> bool {
        self == Kind::PubSeq || self == Kind::UnpubSeq
    }

    pub fn is_unseq(self) -> bool {
        !self.is_seq()
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, Debug)]
pub enum Address {
    PubSeq { name: XorName, tag: u64 },
    PubUnseq { name: XorName, tag: u64 },
    UnpubSeq { name: XorName, tag: u64 },
    UnpubUnseq { name: XorName, tag: u64 },
}

impl Address {
    pub fn from_kind(kind: Kind, name: XorName, tag: u64) -> Self {
        match kind {
            Kind::PubSeq => Address::PubSeq { name, tag },
            Kind::PubUnseq => Address::PubUnseq { name, tag },
            Kind::UnpubSeq => Address::UnpubSeq { name, tag },
            Kind::UnpubUnseq => Address::UnpubUnseq { name, tag },
        }
    }

    pub fn kind(&self) -> Kind {
        match self {
            Address::PubSeq { .. } => Kind::PubSeq,
            Address::PubUnseq { .. } => Kind::PubUnseq,
            Address::UnpubSeq { .. } => Kind::UnpubSeq,
            Address::UnpubUnseq { .. } => Kind::UnpubUnseq,
        }
    }

    pub fn name(&self) -> &XorName {
        match self {
            Address::PubSeq { ref name, .. }
            | Address::PubUnseq { ref name, .. }
            | Address::UnpubSeq { ref name, .. }
            | Address::UnpubUnseq { ref name, .. } => name,
        }
    }

    pub fn tag(&self) -> u64 {
        match self {
            Address::PubSeq { tag, .. }
            | Address::PubUnseq { tag, .. }
            | Address::UnpubSeq { tag, .. }
            | Address::UnpubUnseq { tag, .. } => *tag,
        }
    }

    pub fn is_pub(&self) -> bool {
        self.kind().is_pub()
    }

    pub fn is_unpub(&self) -> bool {
        self.kind().is_unpub()
    }

    pub fn is_seq(&self) -> bool {
        self.kind().is_seq()
    }

    pub fn is_unseq(&self) -> bool {
        self.kind().is_unseq()
    }

    /// Returns the Address serialised and encoded in z-base-32.
    pub fn encode_to_zbase32(&self) -> String {
        utils::encode(&self)
    }

    /// Create from z-base-32 encoded string.
    pub fn decode_from_zbase32<I: Decodable>(encoded: I) -> Result<Self> {
        utils::decode(encoded)
    }
}

/// Object storing an appendonly data variant.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, Debug)]
pub enum Data {
    PubSeq(PubSeqAppendOnlyData),
    PubUnseq(PubUnseqAppendOnlyData),
    UnpubSeq(UnpubSeqAppendOnlyData),
    UnpubUnseq(UnpubUnseqAppendOnlyData),
}

impl Data {
    pub fn check_permission(&self, action: Action, requester: PublicKey) -> Result<()> {
        match (self, action) {
            (Data::PubSeq(_), Action::Read) | (Data::PubUnseq(_), Action::Read) => return Ok(()),
            _ => (),
        }

        match self {
            Data::PubSeq(data) => data.check_permission(requester, action),
            Data::PubUnseq(data) => data.check_permission(requester, action),
            Data::UnpubSeq(data) => data.check_permission(requester, action),
            Data::UnpubUnseq(data) => data.check_permission(requester, action),
        }
    }

    pub fn address(&self) -> &Address {
        match self {
            Data::PubSeq(data) => data.address(),
            Data::PubUnseq(data) => data.address(),
            Data::UnpubSeq(data) => data.address(),
            Data::UnpubUnseq(data) => data.address(),
        }
    }

    pub fn kind(&self) -> Kind {
        self.address().kind()
    }

    pub fn name(&self) -> &XorName {
        self.address().name()
    }

    pub fn tag(&self) -> u64 {
        self.address().tag()
    }

    pub fn is_pub(&self) -> bool {
        self.kind().is_pub()
    }

    pub fn is_unpub(&self) -> bool {
        self.kind().is_unpub()
    }

    pub fn is_seq(&self) -> bool {
        self.kind().is_seq()
    }

    pub fn is_unseq(&self) -> bool {
        self.kind().is_unseq()
    }

    pub fn entries_index(&self) -> u64 {
        match self {
            Data::PubSeq(data) => data.entries_index(),
            Data::PubUnseq(data) => data.entries_index(),
            Data::UnpubSeq(data) => data.entries_index(),
            Data::UnpubUnseq(data) => data.entries_index(),
        }
    }

    pub fn permissions_index(&self) -> u64 {
        match self {
            Data::PubSeq(data) => data.permissions_index(),
            Data::PubUnseq(data) => data.permissions_index(),
            Data::UnpubSeq(data) => data.permissions_index(),
            Data::UnpubUnseq(data) => data.permissions_index(),
        }
    }

    pub fn owners_index(&self) -> u64 {
        match self {
            Data::PubSeq(data) => data.owners_index(),
            Data::PubUnseq(data) => data.owners_index(),
            Data::UnpubSeq(data) => data.owners_index(),
            Data::UnpubUnseq(data) => data.owners_index(),
        }
    }

    pub fn in_range(&self, start: Index, end: Index) -> Option<Entries> {
        match self {
            Data::PubSeq(data) => data.in_range(start, end),
            Data::PubUnseq(data) => data.in_range(start, end),
            Data::UnpubSeq(data) => data.in_range(start, end),
            Data::UnpubUnseq(data) => data.in_range(start, end),
        }
    }

    pub fn get(&self, key: &[u8]) -> Option<&Vec<u8>> {
        match self {
            Data::PubSeq(data) => data.get(key),
            Data::PubUnseq(data) => data.get(key),
            Data::UnpubSeq(data) => data.get(key),
            Data::UnpubUnseq(data) => data.get(key),
        }
    }

    pub fn indices(&self) -> Indices {
        match self {
            Data::PubSeq(data) => data.indices(),
            Data::PubUnseq(data) => data.indices(),
            Data::UnpubSeq(data) => data.indices(),
            Data::UnpubUnseq(data) => data.indices(),
        }
    }

    pub fn last_entry(&self) -> Option<&Entry> {
        match self {
            Data::PubSeq(data) => data.last_entry(),
            Data::PubUnseq(data) => data.last_entry(),
            Data::UnpubSeq(data) => data.last_entry(),
            Data::UnpubUnseq(data) => data.last_entry(),
        }
    }

    pub fn owner(&self, owners_index: impl Into<Index>) -> Option<&Owner> {
        match self {
            Data::PubSeq(data) => data.owner(owners_index),
            Data::PubUnseq(data) => data.owner(owners_index),
            Data::UnpubSeq(data) => data.owner(owners_index),
            Data::UnpubUnseq(data) => data.owner(owners_index),
        }
    }

    pub fn check_is_last_owner(&self, requester: PublicKey) -> Result<()> {
        match self {
            Data::PubSeq(data) => data.check_is_last_owner(requester),
            Data::PubUnseq(data) => data.check_is_last_owner(requester),
            Data::UnpubSeq(data) => data.check_is_last_owner(requester),
            Data::UnpubUnseq(data) => data.check_is_last_owner(requester),
        }
    }

    pub fn pub_user_permissions(
        &self,
        user: User,
        index: impl Into<Index>,
    ) -> Result<PubPermissionSet> {
        self.pub_permissions(index)?
            .permissions()
            .get(&user)
            .cloned()
            .ok_or(Error::NoSuchEntry)
    }

    pub fn unpub_user_permissions(
        &self,
        user: PublicKey,
        index: impl Into<Index>,
    ) -> Result<UnpubPermissionSet> {
        self.unpub_permissions(index)?
            .permissions()
            .get(&user)
            .cloned()
            .ok_or(Error::NoSuchEntry)
    }

    pub fn pub_permissions(&self, index: impl Into<Index>) -> Result<&PubPermissions> {
        let perms = match self {
            Data::PubSeq(data) => data.permissions(index),
            Data::PubUnseq(data) => data.permissions(index),
            _ => return Err(Error::NoSuchData),
        };
        perms.ok_or(Error::NoSuchEntry)
    }

    pub fn unpub_permissions(&self, index: impl Into<Index>) -> Result<&UnpubPermissions> {
        let perms = match self {
            Data::UnpubSeq(data) => data.permissions(index),
            Data::UnpubUnseq(data) => data.permissions(index),
            _ => return Err(Error::NoSuchData),
        };
        perms.ok_or(Error::NoSuchEntry)
    }

    pub fn shell(&self, index: impl Into<Index>) -> Result<Self> {
        match self {
            Data::PubSeq(adata) => adata.shell(index).map(Data::PubSeq),
            Data::PubUnseq(adata) => adata.shell(index).map(Data::PubUnseq),
            Data::UnpubSeq(adata) => adata.shell(index).map(Data::UnpubSeq),
            Data::UnpubUnseq(adata) => adata.shell(index).map(Data::UnpubUnseq),
        }
    }
}

impl From<PubSeqAppendOnlyData> for Data {
    fn from(data: PubSeqAppendOnlyData) -> Self {
        Data::PubSeq(data)
    }
}

impl From<PubUnseqAppendOnlyData> for Data {
    fn from(data: PubUnseqAppendOnlyData) -> Self {
        Data::PubUnseq(data)
    }
}

impl From<UnpubSeqAppendOnlyData> for Data {
    fn from(data: UnpubSeqAppendOnlyData) -> Self {
        Data::UnpubSeq(data)
    }
}

impl From<UnpubUnseqAppendOnlyData> for Data {
    fn from(data: UnpubUnseqAppendOnlyData) -> Self {
        Data::UnpubUnseq(data)
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub struct AppendOperation {
    // Address of an AppendOnlyData object on the network.
    pub address: Address,
    // A list of entries to append.
    pub values: Entries,
}

fn to_absolute_index(index: Index, count: usize) -> Option<usize> {
    match index {
        Index::FromStart(index) if index as usize <= count => Some(index as usize),
        Index::FromStart(_) => None,
        Index::FromEnd(index) => count.checked_sub(index as usize),
    }
}

fn to_absolute_range(start: Index, end: Index, count: usize) -> Option<Range<usize>> {
    let start = to_absolute_index(start, count)?;
    let end = to_absolute_index(end, count)?;

    if start <= end {
        Some(start..end)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use threshold_crypto::SecretKey;
    use unwrap::{unwrap, unwrap_err};

    #[test]
    fn append_permissions() {
        let mut data = UnpubSeqAppendOnlyData::new(XorName([1; 32]), 10000);

        // Append the first permission set with correct indices - should pass.
        let res = data.append_permissions(
            UnpubPermissions {
                permissions: BTreeMap::new(),
                entries_index: 0,
                owners_index: 0,
            },
            0,
        );

        match res {
            Ok(()) => (),
            Err(x) => panic!("Unexpected error: {:?}", x),
        }

        // Verify that the permissions have been added.
        assert_eq!(
            unwrap!(data.permissions_range(Index::FromStart(0), Index::FromEnd(0),)).len(),
            1
        );

        // Append another permissions entry with incorrect indices - should fail.
        let res = data.append_permissions(
            UnpubPermissions {
                permissions: BTreeMap::new(),
                entries_index: 64,
                owners_index: 0,
            },
            1,
        );

        match res {
            Err(_) => (),
            Ok(()) => panic!("Unexpected Ok(()) result"),
        }

        // Verify that the number of permissions has not been changed.
        assert_eq!(
            unwrap!(data.permissions_range(Index::FromStart(0), Index::FromEnd(0),)).len(),
            1
        );
    }

    #[test]
    fn append_owners() {
        let owner_pk = gen_public_key();

        let mut data = UnpubSeqAppendOnlyData::new(XorName([1; 32]), 10000);

        // Append the first owner with correct indices - should pass.
        let res = data.append_owner(
            Owner {
                public_key: owner_pk,
                entries_index: 0,
                permissions_index: 0,
            },
            0,
        );

        match res {
            Ok(()) => (),
            Err(x) => panic!("Unexpected error: {:?}", x),
        }

        // Verify that the owner has been added.
        assert_eq!(
            unwrap!(data.owners_range(Index::FromStart(0), Index::FromEnd(0),)).len(),
            1
        );

        // Append another owners entry with incorrect indices - should fail.
        let res = data.append_owner(
            Owner {
                public_key: owner_pk,
                entries_index: 64,
                permissions_index: 0,
            },
            1,
        );

        match res {
            Err(_) => (),
            Ok(()) => panic!("Unexpected Ok(()) result"),
        }

        // Verify that the number of owners has not been changed.
        assert_eq!(
            unwrap!(data.owners_range(Index::FromStart(0), Index::FromEnd(0),)).len(),
            1
        );
    }

    #[test]
    fn seq_append_entries() {
        let mut data = PubSeqAppendOnlyData::new(XorName([1; 32]), 10000);
        unwrap!(data.append(vec![Entry::new(b"hello".to_vec(), b"world".to_vec())], 0));
    }

    #[test]
    fn assert_shell() {
        let owner_pk = gen_public_key();
        let owner_pk1 = gen_public_key();

        let mut data = UnpubSeqAppendOnlyData::new(XorName([1; 32]), 10000);

        let _ = data.append_owner(
            Owner {
                public_key: owner_pk,
                entries_index: 0,
                permissions_index: 0,
            },
            0,
        );

        let _ = data.append_owner(
            Owner {
                public_key: owner_pk1,
                entries_index: 0,
                permissions_index: 0,
            },
            1,
        );

        assert_eq!(data.owners_index(), unwrap!(data.shell(0)).owners_index());
    }

    #[test]
    fn zbase32_encode_decode_adata_address() {
        let name = XorName(rand::random());
        let address = Address::UnpubSeq { name, tag: 15000 };
        let encoded = address.encode_to_zbase32();
        let decoded = unwrap!(self::Address::decode_from_zbase32(&encoded));
        assert_eq!(address, decoded);
    }

    #[test]
    fn append_unseq_data_test() {
        let mut data = UnpubUnseqAppendOnlyData::new(XorName(rand::random()), 10);

        // Assert that the entries are not appended because of duplicate keys.
        let entries = vec![
            Entry::new(b"KEY1".to_vec(), b"VALUE1".to_vec()),
            Entry::new(b"KEY2".to_vec(), b"VALUE2".to_vec()),
            Entry::new(b"KEY1".to_vec(), b"VALUE1".to_vec()),
        ];
        assert_eq!(Error::DuplicateEntryKeys, unwrap_err!(data.append(entries)));

        // Assert that the entries are appended because there are no duplicate keys.
        let entries1 = vec![
            Entry::new(b"KEY1".to_vec(), b"VALUE1".to_vec()),
            Entry::new(b"KEY2".to_vec(), b"VALUE2".to_vec()),
        ];

        unwrap!(data.append(entries1));

        // Assert that entries are not appended because they duplicate some keys appended previously.
        let entries2 = vec![Entry::new(b"KEY2".to_vec(), b"VALUE2".to_vec())];
        assert_eq!(
            Error::KeysExist(entries2.clone()),
            unwrap_err!(data.append(entries2))
        );

        // Assert that no duplicate keys are present and the append operation is successful.
        let entries3 = vec![Entry::new(b"KEY3".to_vec(), b"VALUE3".to_vec())];
        unwrap!(data.append(entries3));
    }

    #[test]
    fn append_seq_data_test() {
        let mut data = UnpubSeqAppendOnlyData::new(XorName(rand::random()), 10);

        // Assert that the entries are not appended because of duplicate keys.
        let entries = vec![
            Entry::new(b"KEY1".to_vec(), b"VALUE1".to_vec()),
            Entry::new(b"KEY2".to_vec(), b"VALUE2".to_vec()),
            Entry::new(b"KEY1".to_vec(), b"VALUE1".to_vec()),
        ];
        assert_eq!(
            Error::DuplicateEntryKeys,
            unwrap_err!(data.append(entries, 0))
        );

        // Assert that the entries are appended because there are no duplicate keys.
        let entries1 = vec![
            Entry::new(b"KEY1".to_vec(), b"VALUE1".to_vec()),
            Entry::new(b"KEY2".to_vec(), b"VALUE2".to_vec()),
        ];
        unwrap!(data.append(entries1, 0));

        // Assert that entries are not appended because they duplicate some keys appended previously.
        let entries2 = vec![Entry::new(b"KEY2".to_vec(), b"VALUE2".to_vec())];
        assert_eq!(
            Error::KeysExist(entries2.clone()),
            unwrap_err!(data.append(entries2, 2))
        );

        // Assert that no duplicate keys are present and the append operation is successful.
        let entries3 = vec![Entry::new(b"KEY3".to_vec(), b"VALUE3".to_vec())];
        unwrap!(data.append(entries3, 2));
    }

    #[test]
    fn in_range() {
        let mut data = PubSeqAppendOnlyData::new(rand::random(), 10);
        let entries = vec![
            Entry::new(b"key0".to_vec(), b"value0".to_vec()),
            Entry::new(b"key1".to_vec(), b"value1".to_vec()),
        ];
        unwrap!(data.append(entries, 0));

        assert_eq!(
            data.in_range(Index::FromStart(0), Index::FromStart(0)),
            Some(vec![])
        );
        assert_eq!(
            data.in_range(Index::FromStart(0), Index::FromStart(1)),
            Some(vec![Entry::new(b"key0".to_vec(), b"value0".to_vec())])
        );
        assert_eq!(
            data.in_range(Index::FromStart(0), Index::FromStart(2)),
            Some(vec![
                Entry::new(b"key0".to_vec(), b"value0".to_vec()),
                Entry::new(b"key1".to_vec(), b"value1".to_vec())
            ])
        );

        assert_eq!(
            data.in_range(Index::FromEnd(2), Index::FromEnd(1)),
            Some(vec![Entry::new(b"key0".to_vec(), b"value0".to_vec()),])
        );
        assert_eq!(
            data.in_range(Index::FromEnd(2), Index::FromEnd(0)),
            Some(vec![
                Entry::new(b"key0".to_vec(), b"value0".to_vec()),
                Entry::new(b"key1".to_vec(), b"value1".to_vec())
            ])
        );

        assert_eq!(
            data.in_range(Index::FromStart(0), Index::FromEnd(0)),
            Some(vec![
                Entry::new(b"key0".to_vec(), b"value0".to_vec()),
                Entry::new(b"key1".to_vec(), b"value1".to_vec())
            ])
        );

        // start > end
        assert_eq!(
            data.in_range(Index::FromStart(1), Index::FromStart(0)),
            None
        );
        assert_eq!(data.in_range(Index::FromEnd(1), Index::FromEnd(2)), None);

        // overflow
        assert_eq!(
            data.in_range(Index::FromStart(0), Index::FromStart(3)),
            None
        );
        assert_eq!(data.in_range(Index::FromEnd(3), Index::FromEnd(0)), None);
    }

    #[test]
    fn get_permissions() {
        let public_key = gen_public_key();
        let invalid_public_key = gen_public_key();

        let mut pub_perms = PubPermissions {
            permissions: BTreeMap::new(),
            entries_index: 0,
            owners_index: 0,
        };
        let _ = pub_perms
            .permissions
            .insert(User::Key(public_key), PubPermissionSet::new(false, false));

        let mut unpub_perms = UnpubPermissions {
            permissions: BTreeMap::new(),
            entries_index: 0,
            owners_index: 0,
        };
        let _ = unpub_perms
            .permissions
            .insert(public_key, UnpubPermissionSet::new(false, false, false));

        // pub, unseq
        let mut data = PubUnseqAppendOnlyData::new(rand::random(), 20);
        unwrap!(data.append_permissions(pub_perms.clone(), 0));
        let data = Data::from(data);

        assert_eq!(data.pub_permissions(0), Ok(&pub_perms));
        assert_eq!(data.unpub_permissions(0), Err(Error::NoSuchData));

        assert_eq!(
            data.pub_user_permissions(User::Key(public_key), 0),
            Ok(PubPermissionSet::new(false, false))
        );
        assert_eq!(
            data.unpub_user_permissions(public_key, 0),
            Err(Error::NoSuchData)
        );
        assert_eq!(
            data.pub_user_permissions(User::Key(invalid_public_key), 0),
            Err(Error::NoSuchEntry)
        );

        // pub, seq
        let mut data = PubSeqAppendOnlyData::new(rand::random(), 20);
        unwrap!(data.append_permissions(pub_perms.clone(), 0));
        let data = Data::from(data);

        assert_eq!(data.pub_permissions(0), Ok(&pub_perms));
        assert_eq!(data.unpub_permissions(0), Err(Error::NoSuchData));

        assert_eq!(
            data.pub_user_permissions(User::Key(public_key), 0),
            Ok(PubPermissionSet::new(false, false))
        );
        assert_eq!(
            data.unpub_user_permissions(public_key, 0),
            Err(Error::NoSuchData)
        );
        assert_eq!(
            data.pub_user_permissions(User::Key(invalid_public_key), 0),
            Err(Error::NoSuchEntry)
        );

        // unpub, unseq
        let mut data = UnpubUnseqAppendOnlyData::new(rand::random(), 20);
        unwrap!(data.append_permissions(unpub_perms.clone(), 0));
        let data = Data::from(data);

        assert_eq!(data.unpub_permissions(0), Ok(&unpub_perms));
        assert_eq!(data.pub_permissions(0), Err(Error::NoSuchData));

        assert_eq!(
            data.unpub_user_permissions(public_key, 0),
            Ok(UnpubPermissionSet::new(false, false, false))
        );
        assert_eq!(
            data.pub_user_permissions(User::Key(public_key), 0),
            Err(Error::NoSuchData)
        );
        assert_eq!(
            data.unpub_user_permissions(invalid_public_key, 0),
            Err(Error::NoSuchEntry)
        );

        // unpub, seq
        let mut data = UnpubSeqAppendOnlyData::new(rand::random(), 20);
        unwrap!(data.append_permissions(unpub_perms.clone(), 0));
        let data = Data::from(data);

        assert_eq!(data.unpub_permissions(0), Ok(&unpub_perms));
        assert_eq!(data.pub_permissions(0), Err(Error::NoSuchData));

        assert_eq!(
            data.unpub_user_permissions(public_key, 0),
            Ok(UnpubPermissionSet::new(false, false, false))
        );
        assert_eq!(
            data.pub_user_permissions(User::Key(public_key), 0),
            Err(Error::NoSuchData)
        );
        assert_eq!(
            data.unpub_user_permissions(invalid_public_key, 0),
            Err(Error::NoSuchEntry)
        );
    }

    fn gen_public_key() -> PublicKey {
        PublicKey::Bls(SecretKey::random().public_key())
    }

    #[test]
    fn check_pub_permission() {
        let public_key_0 = gen_public_key();
        let public_key_1 = gen_public_key();
        let public_key_2 = gen_public_key();
        let mut inner = PubSeqAppendOnlyData::new(XorName([1; 32]), 100);

        // no owner
        let data = Data::from(inner.clone());
        assert_eq!(
            data.check_permission(Action::Append, public_key_0),
            Err(Error::InvalidOwners)
        );
        // data is published - read always allowed
        assert_eq!(data.check_permission(Action::Read, public_key_0), Ok(()));

        // no permissions
        unwrap!(inner.append_owner(
            Owner {
                public_key: public_key_0,
                entries_index: 0,
                permissions_index: 0,
            },
            0,
        ));
        let data = Data::from(inner.clone());

        assert_eq!(data.check_permission(Action::Append, public_key_0), Ok(()));
        assert_eq!(
            data.check_permission(Action::Append, public_key_1),
            Err(Error::InvalidPermissions)
        );
        // data is published - read always allowed
        assert_eq!(data.check_permission(Action::Read, public_key_0), Ok(()));
        assert_eq!(data.check_permission(Action::Read, public_key_1), Ok(()));

        // with permissions
        let mut permissions = PubPermissions {
            permissions: BTreeMap::new(),
            entries_index: 0,
            owners_index: 1,
        };
        let _ = permissions
            .permissions
            .insert(User::Anyone, PubPermissionSet::new(true, false));
        let _ = permissions
            .permissions
            .insert(User::Key(public_key_1), PubPermissionSet::new(None, true));
        unwrap!(inner.append_permissions(permissions, 0));
        let data = Data::from(inner);

        // existing key fallback
        assert_eq!(data.check_permission(Action::Append, public_key_1), Ok(()));
        // existing key override
        assert_eq!(
            data.check_permission(Action::ManagePermissions, public_key_1),
            Ok(())
        );
        // non-existing keys are handled by `Anyone`
        assert_eq!(data.check_permission(Action::Append, public_key_2), Ok(()));
        assert_eq!(
            data.check_permission(Action::ManagePermissions, public_key_2),
            Err(Error::AccessDenied)
        );
        // data is published - read always allowed
        assert_eq!(data.check_permission(Action::Read, public_key_0), Ok(()));
        assert_eq!(data.check_permission(Action::Read, public_key_1), Ok(()));
        assert_eq!(data.check_permission(Action::Read, public_key_2), Ok(()));
    }

    #[test]
    fn check_unpub_permission() {
        let public_key_0 = gen_public_key();
        let public_key_1 = gen_public_key();
        let public_key_2 = gen_public_key();
        let mut inner = UnpubSeqAppendOnlyData::new(XorName([1; 32]), 100);

        // no owner
        let data = Data::from(inner.clone());
        assert_eq!(
            data.check_permission(Action::Read, public_key_0),
            Err(Error::InvalidOwners)
        );

        // no permissions
        unwrap!(inner.append_owner(
            Owner {
                public_key: public_key_0,
                entries_index: 0,
                permissions_index: 0,
            },
            0,
        ));
        let data = Data::from(inner.clone());

        assert_eq!(data.check_permission(Action::Read, public_key_0), Ok(()));
        assert_eq!(
            data.check_permission(Action::Read, public_key_1),
            Err(Error::InvalidPermissions)
        );

        // with permissions
        let mut permissions = UnpubPermissions {
            permissions: BTreeMap::new(),
            entries_index: 0,
            owners_index: 1,
        };
        let _ = permissions
            .permissions
            .insert(public_key_1, UnpubPermissionSet::new(true, true, false));
        unwrap!(inner.append_permissions(permissions, 0));
        let data = Data::from(inner);

        // existing key
        assert_eq!(data.check_permission(Action::Read, public_key_1), Ok(()));
        assert_eq!(data.check_permission(Action::Append, public_key_1), Ok(()));
        assert_eq!(
            data.check_permission(Action::ManagePermissions, public_key_1),
            Err(Error::AccessDenied)
        );

        // non-existing key
        assert_eq!(
            data.check_permission(Action::Read, public_key_2),
            Err(Error::InvalidPermissions)
        );
        assert_eq!(
            data.check_permission(Action::Append, public_key_2),
            Err(Error::InvalidPermissions)
        );
        assert_eq!(
            data.check_permission(Action::ManagePermissions, public_key_2),
            Err(Error::InvalidPermissions)
        );
    }
}
