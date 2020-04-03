//! Data types for contract metadata (including version and method type signatures)

use crate::{
    bytesrepr::{self, FromBytes, ToBytes},
    uref::URef,
    CLType, CLTyped, ProtocolVersion, SemVer,
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
    vec::Vec,
};
use core::convert::TryInto;
use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::{FromPrimitive, ToPrimitive};

/// Set of errors which may happen when working with contract headers.
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive)]
#[repr(u8)]
pub enum Error {
    /// Attempt to add/remove contract versions without the right access key.
    InvalidAccessKey = 1,
    /// Attempt to override an existing or previously existing version with a
    /// new header (this is not allowed to ensure immutability of a given
    /// version).
    PreviouslyUsedVersion = 2,
    /// Attempted to remove a version that does not exist.
    VersionNotFound = 3,
}

impl Error {
    /// Convert to byte for serialization purposes.
    pub fn to_u8(self) -> u8 {
        ToPrimitive::to_u8(&self).unwrap()
    }

    /// Construct from byte (for serialization purposes).
    pub fn from_i32(x: i32) -> Option<Self> {
        let y: u8 = x.try_into().ok()?;
        FromPrimitive::from_u8(y)
    }
}

/// Collection of different versions of the same contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractMetadata {
    /// Key used to add or remove versions
    access_key: URef,
    /// Versions that can be called
    active_versions: BTreeMap<SemVer, ContractHeader>,
    /// Old versions that are no longer supported
    removed_versions: BTreeSet<SemVer>,
}

impl ContractMetadata {
    /// Create new `ContractMetadata` (with no versions) from given access key.
    pub fn new(access_key: URef) -> Self {
        ContractMetadata {
            access_key,
            active_versions: BTreeMap::new(),
            removed_versions: BTreeSet::new(),
        }
    }

    /// Get the access key for this ContractMetadata.
    pub fn access_key(&self) -> URef {
        self.access_key
    }

    /// Get the contract header for the given version (if present)
    pub fn get_version(mut self, version: &SemVer) -> Option<ContractHeader> {
        self.active_versions.remove(version)
    }

    /// Modify the collection of active versions to include the given one.
    pub fn with_version(&mut self, version: SemVer, header: ContractHeader) -> Result<(), Error> {
        if self.removed_versions.contains(&version) || self.active_versions.contains_key(&version) {
            return Err(Error::PreviouslyUsedVersion);
        }

        self.active_versions.insert(version, header);
        Ok(())
    }

    /// Remove the given version from active versions, putting it into removed versions.
    pub fn remove_version(&mut self, version: SemVer) -> Result<(), Error> {
        if self.removed_versions.contains(&version) {
            return Ok(());
        } else if !self.active_versions.contains_key(&version) {
            return Err(Error::VersionNotFound);
        }

        self.active_versions.remove(&version);
        self.removed_versions.insert(version);
        Ok(())
    }
}

impl CLTyped for ContractMetadata {
    fn cl_type() -> CLType {
        CLType::ContractMetadata
    }
}

impl ToBytes for ContractMetadata {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        let mut result = bytesrepr::allocate_buffer(self)?;

        result.append(&mut self.access_key.to_bytes()?);
        result.append(&mut self.active_versions.to_bytes()?);
        result.append(&mut self.removed_versions.to_bytes()?);

        Ok(result)
    }

    fn serialized_length(&self) -> usize {
        self.access_key.serialized_length()
            + self.active_versions.serialized_length()
            + self.removed_versions.serialized_length()
    }
}

impl FromBytes for ContractMetadata {
    fn from_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), bytesrepr::Error> {
        let (access_key, bytes) = URef::from_bytes(bytes)?;
        let (active_versions, bytes) = BTreeMap::<SemVer, ContractHeader>::from_bytes(bytes)?;
        let (removed_versions, bytes) = BTreeSet::<SemVer>::from_bytes(bytes)?;
        let result = ContractMetadata {
            access_key,
            active_versions,
            removed_versions,
        };

        Ok((result, bytes))
    }
}

/// Methods and type signatures supported by a contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractHeader {
    methods: BTreeMap<String, EntryPoint>,
    protocol_version: ProtocolVersion,
}

impl ContractHeader {
    /// `ContractHeader` constructor.
    pub fn new(methods: BTreeMap<String, EntryPoint>, protocol_version: ProtocolVersion) -> Self {
        ContractHeader {
            methods,
            protocol_version,
        }
    }

    /// Checks whether there is a method with the given name
    pub fn has_method_name(&self, name: &str) -> bool {
        self.methods.contains_key(name)
    }

    /// Returns the list of method names
    pub fn method_names(&self) -> Vec<&str> {
        self.methods.keys().map(|s| s.as_str()).collect()
    }

    /// Returns the type signature for the given `method`.
    pub fn get_method(mut self, method: &String) -> Option<EntryPoint> {
        self.methods.remove(method)
    }

    /// Get the protocol version this header is targeting.
    pub fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
}

impl ToBytes for ContractHeader {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        let mut result = ToBytes::to_bytes(&self.methods)?;
        result.append(&mut self.protocol_version.to_bytes()?);
        Ok(result)
    }

    fn serialized_length(&self) -> usize {
        ToBytes::serialized_length(&self.methods)
            + ToBytes::serialized_length(&self.protocol_version)
    }
}

impl FromBytes for ContractHeader {
    fn from_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), bytesrepr::Error> {
        let (methods, bytes) = BTreeMap::<String, EntryPoint>::from_bytes(bytes)?;
        let (protocol_version, bytes) = ProtocolVersion::from_bytes(bytes)?;
        Ok((
            ContractHeader {
                methods,
                protocol_version,
            },
            bytes,
        ))
    }
}

/// Type signature of a method. Order of arguments matter since can be
/// referenced by index as well as name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryPoint {
    // TODO: I wonder if we could have access-controlled methods
    args: Vec<Arg>,
    ret: CLType,
}

impl EntryPoint {
    /// `EntryPoint` constructor.
    pub fn new(args: Vec<Arg>, ret: CLType) -> Self {
        EntryPoint { args, ret }
    }

    /// Get the arguments for this method.
    pub fn args(&self) -> &[Arg] {
        self.args.as_slice()
    }
}

impl ToBytes for EntryPoint {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        let mut result = ToBytes::to_bytes(&self.args)?;
        self.ret.append_bytes(&mut result);

        Ok(result)
    }

    fn serialized_length(&self) -> usize {
        ToBytes::serialized_length(&self.args) + self.ret.serialized_length()
    }
}

impl FromBytes for EntryPoint {
    fn from_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), bytesrepr::Error> {
        let (args, bytes) = Vec::<Arg>::from_bytes(bytes)?;
        let (ret, bytes) = CLType::from_bytes(bytes)?;

        Ok((EntryPoint { args, ret }, bytes))
    }
}

/// Argument to a method
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arg {
    name: String,
    cl_type: CLType,
}

impl Arg {
    /// `Arg` constructor.
    pub fn new(name: String, cl_type: CLType) -> Self {
        Arg { name, cl_type }
    }

    /// Get the type of this argument.
    pub fn cl_type(&self) -> &CLType {
        &self.cl_type
    }
}

impl ToBytes for Arg {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        let mut result = ToBytes::to_bytes(&self.name)?;
        self.cl_type.append_bytes(&mut result);

        Ok(result)
    }

    fn serialized_length(&self) -> usize {
        ToBytes::serialized_length(&self.name) + self.cl_type.serialized_length()
    }
}

impl FromBytes for Arg {
    fn from_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), bytesrepr::Error> {
        let (name, bytes) = String::from_bytes(bytes)?;
        let (cl_type, bytes) = CLType::from_bytes(bytes)?;

        Ok((Arg { name, cl_type }, bytes))
    }
}