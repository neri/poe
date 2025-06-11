//! Zero Allocation Device Tree Parser
#![cfg_attr(not(test), no_std)]

use core::{
    ffi::c_void,
    fmt,
    marker::PhantomData,
    mem::transmute,
    ptr::null,
    slice::{self, Iter},
    str,
};

#[cfg(feature = "uuid")]
use uuid::{Guid, guid};

/// EFI GUID of the Device Tree Table
#[cfg(feature = "uuid")]
pub const DTB_TABLE_GUID: Guid = guid!("b1b621d5-f19c-41a5-830b-d9152c69aae0");

pub struct DeviceTree<'a> {
    header: &'a Header,
    root_node: RootNode<'a>,
}

impl DeviceTree<'_> {
    pub const FDT_BEGIN_NODE: u32 = 1;
    pub const FDT_END_NODE: u32 = 2;
    pub const FDT_PROP: u32 = 3;
    pub const FDT_NOP: u32 = 4;
    pub const FDT_END: u32 = 9;

    pub unsafe fn parse<'a>(ptr: *const u8) -> Result<DeviceTree<'a>, ParseError> {
        if ptr == null() {
            return Err(ParseError::InvalidInput);
        }
        let header = unsafe { &*(ptr as *const Header) };
        if header.is_valid() {
            let mut tokens = FdtTokens::new(header);
            if let Some(Token::BeginNode(NodeName::ROOT)) = tokens.next() {
                let root_node = RootNode::new(tokens);
                return Ok(DeviceTree { header, root_node });
            }
        }
        Err(ParseError::InvalidData)
    }

    #[inline]
    pub fn from_slice<'a>(slice: &'a [u8]) -> Result<DeviceTree<'a>, ParseError> {
        unsafe { Self::parse(slice.as_ptr()) }
    }

    #[inline]
    pub fn as_ptr(&self) -> *const c_void {
        self.header as *const _ as *const c_void
    }

    #[inline]
    pub fn into_ptr(self) -> *const c_void {
        self.as_ptr()
    }

    #[inline]
    pub const fn header(&self) -> &Header {
        self.header
    }

    #[inline]
    pub fn root(&self) -> &RootNode<'_> {
        &self.root_node
    }

    #[inline]
    pub fn range(&self) -> (*const c_void, usize) {
        (
            self.header() as *const _ as *const c_void,
            self.header().total_size(),
        )
    }

    pub fn memory_map(&self) -> Option<impl Iterator<Item = (u64, u64)>> {
        unsafe {
            let root = self.root();
            let address_cells = root.address_cells();
            let size_cells = root.size_cells();
            let memory = match root.memory() {
                Some(v) => v,
                None => return None,
            };
            let slice = match memory.get_prop(PropName::REG) {
                Some(prop) => slice::from_raw_parts(prop.ptr(), prop.len() / 4),
                None => return None,
            };
            Some(AddressAndSizeIter::new(slice, address_cells, size_cells))
        }
    }

    pub fn reserved_memory_map(&self) -> Option<Node<'_>> {
        let root = self.root();
        root.find_first_child(NodeName::RESERVED_MEMORY)
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
pub enum ParseError {
    InvalidInput,
    InvalidData,
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq)]
pub struct BeU32(u32);

impl BeU32 {
    #[inline]
    pub const fn new(val: u32) -> Self {
        Self(val.to_be())
    }

    #[inline]
    pub const fn as_u32(&self) -> u32 {
        self.to_be()
    }

    #[inline]
    pub const fn to_be(&self) -> u32 {
        self.0.to_be()
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq)]
pub struct BeU64(u64);

impl BeU64 {
    #[inline]
    pub const fn new(val: u64) -> Self {
        Self(val.to_be())
    }

    #[inline]
    pub const fn as_u64(&self) -> u64 {
        self.to_be()
    }

    #[inline]
    pub const fn to_be(&self) -> u64 {
        self.0.to_be()
    }
}

#[repr(C)]
pub struct Header {
    magic: BeU32,
    totalsize: BeU32,
    off_dt_struct: BeU32,
    off_dt_strings: BeU32,
    off_mem_rsvmap: BeU32,
    version: BeU32,
    last_comp_version: BeU32,
    boot_cpuid_phys: BeU32,
    size_dt_string: BeU32,
    size_dt_struct: BeU32,
}

impl Header {
    pub const MAGIC: u32 = 0xD00DFEED;
    pub const CURRENT_VERSION: u32 = 0x11;
    pub const COMPATIBLE_VERSION: u32 = 0x10;

    #[inline]
    pub const fn is_valid(&self) -> bool {
        self.magic() == Self::MAGIC
            && self.version() == Self::CURRENT_VERSION
            && self.last_comp_version() == Self::COMPATIBLE_VERSION
    }

    #[inline]
    pub const fn magic(&self) -> u32 {
        self.magic.to_be()
    }

    #[inline]
    pub fn as_ptr(&self) -> *const Self {
        self as *const _
    }

    #[inline]
    pub const fn total_size(&self) -> usize {
        self.totalsize.to_be() as usize
    }

    #[inline]
    pub const fn off_dt_struct(&self) -> usize {
        self.off_dt_struct.to_be() as usize
    }

    #[inline]
    pub const fn off_dt_strings(&self) -> usize {
        self.off_dt_strings.to_be() as usize
    }

    #[inline]
    pub const fn off_mem_rsvmap(&self) -> usize {
        self.off_mem_rsvmap.to_be() as usize
    }

    #[inline]
    pub const fn version(&self) -> u32 {
        self.version.to_be()
    }

    #[inline]
    pub const fn last_comp_version(&self) -> u32 {
        self.last_comp_version.to_be()
    }

    #[inline]
    pub fn struct_ptr(&self) -> *const BeU32 {
        let p = self as *const Self as *const u8;
        unsafe { p.add(self.off_dt_struct()) as *const _ }
    }

    #[inline]
    pub fn string_ptr(&self) -> *const u8 {
        let p = self as *const Self as *const u8;
        unsafe { p.add(self.off_dt_strings()) }
    }

    #[inline]
    pub fn reserve_map_ptr(&self) -> *const BeU64 {
        let p = self as *const Self as *const u8;
        unsafe { p.add(self.off_mem_rsvmap()) as *const _ }
    }

    #[inline]
    pub fn reserved_maps(&self) -> impl Iterator<Item = (u64, u64)> + '_ {
        FdtRsvMapIter {
            header: self,
            index: 0,
        }
    }

    #[inline]
    pub fn tokens<'a>(&'a self) -> impl Iterator<Item = Token<'a>> {
        FdtTokens::new(self)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Token<'a> {
    BeginNode(NodeName<'a>),
    EndNode,
    Prop(PropName<'a>, *const c_void, usize),
}

struct FdtTokens<'a> {
    header: &'a Header,
    index: usize,
}

impl<'a> FdtTokens<'a> {
    #[inline]
    pub const fn new(header: &'a Header) -> FdtTokens<'a> {
        Self { header, index: 0 }
    }
}

impl<'a> FdtTokens<'a> {
    #[inline]
    pub fn fork(&self) -> FdtTokens<'a> {
        Self {
            header: self.header,
            index: self.index,
        }
    }
}

impl<'a> Iterator for FdtTokens<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            let mut index = self.index;
            let mut ptr = self.header.struct_ptr().add(index);
            let result = loop {
                let token = ptr.read_volatile().to_be();
                match token {
                    DeviceTree::FDT_NOP => {
                        ptr = ptr.add(1);
                        index += 1;
                    }
                    DeviceTree::FDT_BEGIN_NODE => {
                        let p = ptr.add(1) as *const u8;
                        let len = _c_strlen(p, 255);
                        let name = NodeName(_c_string(p, len));
                        index += 1 + (len + 4) / 4;
                        break Token::BeginNode(name);
                    }
                    DeviceTree::FDT_PROP => {
                        let data_len = ptr.add(1).read_volatile().to_be() as usize;
                        let name_ptr = ptr.add(2).read_volatile().to_be() as usize;
                        let name = PropName(_c_string(self.header.string_ptr().add(name_ptr), 255));
                        index += 3 + ((data_len + 3) / 4);
                        break Token::Prop(name, ptr.add(3) as *const c_void, data_len);
                    }
                    DeviceTree::FDT_END_NODE => {
                        index += 1;
                        break Token::EndNode;
                    }
                    // DeviceTree::FDT_END
                    _ => return None,
                }
            };
            self.index = index;
            Some(result)
        }
    }
}

pub trait FdtNode {
    fn node<'a>(&'a self) -> &'a Node<'a>;

    #[inline]
    fn name(&self) -> NodeName<'_> {
        self.node().name
    }

    #[inline]
    fn props(&self) -> FdtProps<'_> {
        FdtProps::new(self.node().tokens())
    }

    fn get_prop(&self, prop_name: PropName) -> Option<FdtProperty<'_>> {
        for prop in self.props() {
            if prop.name() == prop_name {
                return Some(prop);
            }
        }
        None
    }

    #[inline]
    fn get_prop_str(&self, prop_name: PropName) -> Option<&str> {
        self.get_prop(prop_name).map(|v| v.as_str())
    }

    #[inline]
    fn get_prop_u32(&self, prop_name: PropName) -> Option<u32> {
        self.get_prop(prop_name).and_then(|v| v.as_u32())
    }

    fn find_first_child(&self, prefix: NodeName) -> Option<Node<'_>> {
        let mut level = 0;
        let mut iter = self.node().tokens();
        while let Some(token) = iter.next() {
            match token {
                Token::BeginNode(name) => {
                    if level == 0 && name.without_unit() == prefix {
                        let address_cells = self.node().address_cells().unwrap_or(0);
                        let size_cells = self.node().size_cells().unwrap_or(0);
                        return Some(Node::new(iter, name, address_cells, size_cells));
                    }
                    level += 1;
                }
                Token::Prop(_, _, _) => continue,
                Token::EndNode => {
                    if level == 0 {
                        break;
                    }
                    level -= 1;
                }
            }
        }
        None
    }

    fn find_child_exact(&self, node_name: NodeName) -> Option<Node<'_>> {
        let mut level = 0;
        let mut iter = self.node().tokens();
        while let Some(token) = iter.next() {
            match token {
                Token::BeginNode(name) => {
                    if level == 0 && name == node_name {
                        let address_cells = self.node().address_cells().unwrap_or(0);
                        let size_cells = self.node().size_cells().unwrap_or(0);
                        return Some(Node::new(iter, name, address_cells, size_cells));
                    }
                    level += 1;
                }
                Token::Prop(_, _, _) => continue,
                Token::EndNode => {
                    if level == 0 {
                        break;
                    }
                    level -= 1;
                }
            }
        }
        None
    }

    #[inline]
    fn children(&self) -> FdtChildNodes<'_> {
        let address_cells = self.node().address_cells().unwrap_or(0);
        let size_cells = self.node().size_cells().unwrap_or(0);
        FdtChildNodes::new(self.node().tokens(), address_cells, size_cells)
    }
}

pub struct Node<'a> {
    header: &'a Header,
    index: usize,
    name: NodeName<'a>,
    address_cells: u32,
    size_cells: u32,
}

impl FdtNode for Node<'_> {
    #[inline]
    fn node(&self) -> &Node<'_> {
        self
    }
}

impl<'a> Node<'a> {
    #[inline]
    const fn new(
        iter: FdtTokens<'a>,
        name: NodeName<'a>,
        address_cells: u32,
        size_cells: u32,
    ) -> Node<'a> {
        Self {
            header: iter.header,
            index: iter.index,
            name,
            address_cells,
            size_cells,
        }
    }

    #[inline]
    fn tokens(&'a self) -> FdtTokens<'a> {
        FdtTokens {
            header: self.header,
            index: self.index,
        }
    }

    /// Well-known property name `#address-cells`
    #[inline]
    pub fn address_cells(&self) -> Option<u32> {
        self.get_prop_u32(PropName::ADDRESS_CELLS)
    }

    /// Well-known property name `#size-cells`
    #[inline]
    pub fn size_cells(&self) -> Option<u32> {
        self.get_prop_u32(PropName::SIZE_CELLS)
    }

    /// Well-known property name `compatible`
    pub fn compatible(&self) -> Option<impl Iterator<Item = &str>> {
        self.get_prop(PropName::COMPATIBLE).map(|v| v.string_list())
    }

    pub fn is_compatible_with(&self, target: &str) -> bool {
        let Some(mut compatible) = self.compatible() else {
            return false;
        };
        compatible.any(|v| v == target)
    }

    /// Well-known property name `reg`
    pub fn reg(&'a self) -> Option<impl Iterator<Item = (u64, u64)> + 'a> {
        self.get_prop(PropName::REG)
            .map(|v| AddressAndSizeIter::new(v.words(), self.address_cells, self.size_cells))
    }

    /// Well-known property name `status`
    #[inline]
    pub fn status(&self) -> Result<Status, Option<&str>> {
        match self.get_prop_str(PropName::STATUS) {
            Some("okay") => Ok(Status::Okay),
            Some("disabled") => Ok(Status::Disabled),
            Some("reserved") => Ok(Status::Reserved),
            Some("fail") => Ok(Status::Fail),
            Some("fail-sss") => Ok(Status::FailSss),
            Some(v) => Err(Some(v)),
            None => Err(None),
        }
    }

    #[inline]
    pub fn status_is_ok(&self) -> bool {
        match self.status() {
            Ok(Status::Okay) => true,
            Err(None) => true,
            _ => false,
        }
    }

    pub fn ranges(&'a self) -> Option<impl Iterator<Item = RangeTriple> + 'a> {
        let ranges = self.get_prop(PropName::RANGES)?;
        let parent_address_cells = self.address_cells;
        let child_address_cells = self.address_cells()?;
        let size_cells = self.size_cells()?;

        Some(RangeTripleIter {
            iter: ranges.words().iter(),
            parent_address_cells,
            child_address_cells,
            size_cells,
        })
    }
}

pub struct RootNode<'a> {
    node: Node<'a>,
}

impl FdtNode for RootNode<'_> {
    #[inline]
    fn node(&self) -> &Node<'_> {
        &self.node
    }
}

impl<'a> RootNode<'a> {
    #[inline]
    fn new(iter: FdtTokens<'a>) -> Self {
        Self {
            node: Node::new(iter, NodeName::ROOT, 0, 0),
        }
    }
}

impl RootNode<'_> {
    #[inline]
    pub fn address_cells(&self) -> u32 {
        self.node().address_cells().unwrap_or_default()
    }

    #[inline]
    pub fn size_cells(&self) -> u32 {
        self.node().size_cells().unwrap_or_default()
    }

    #[inline]
    pub fn model(&self) -> &str {
        self.node()
            .get_prop_str(PropName::MODEL)
            .unwrap_or_default()
    }

    #[inline]
    pub fn compatible(&self) -> Option<impl Iterator<Item = &str>> {
        self.node().compatible()
    }

    #[inline]
    pub fn serial_number(&self) -> Option<&str> {
        self.node().get_prop_str(PropName::SERIAL_NUMBER)
    }

    /// TODO: multiple nodes
    #[inline]
    pub fn aliases(&self) -> Option<Node<'_>> {
        self.node().find_first_child(NodeName::ALIASES)
    }

    /// TODO: multiple nodes
    #[inline]
    pub fn memory(&self) -> Option<Node<'_>> {
        self.node().find_first_child(NodeName::MEMORY)
    }

    #[inline]
    pub fn reserved_memory(&self) -> Option<Node<'_>> {
        self.node().find_first_child(NodeName::RESERVED_MEMORY)
    }

    #[inline]
    pub fn chosen(&self) -> Option<ChosenNode<'_>> {
        self.node()
            .find_first_child(NodeName::CHOSEN)
            .map(|node| ChosenNode { node })
    }

    #[inline]
    pub fn cpus(&self) -> Option<Node<'_>> {
        self.node().find_first_child(NodeName::CPUS)
    }
}

pub struct CpusNode<'a> {
    node: Node<'a>,
}

impl FdtNode for CpusNode<'_> {
    #[inline]
    fn node(&self) -> &Node<'_> {
        &self.node
    }
}

impl CpusNode<'_> {
    pub fn cpus(&self) -> ! {
        todo!()
    }
}

pub struct ChosenNode<'a> {
    node: Node<'a>,
}

impl FdtNode for ChosenNode<'_> {
    #[inline]
    fn node(&self) -> &Node<'_> {
        &self.node
    }
}

impl ChosenNode<'_> {
    #[inline]
    pub fn bootargs(&self) -> Option<&str> {
        self.node.get_prop_str(PropName::BOOTARGS)
    }

    #[inline]
    pub fn stdout_path(&self) -> Option<&str> {
        self.node.get_prop_str(PropName::STDOUT_PATH)
    }

    #[inline]
    pub fn stdin_path(&self) -> Option<&str> {
        self.node.get_prop_str(PropName::STDIN_PATH)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeName<'a>(pub &'a str);

impl NodeName<'_> {
    /// Well-known node name `/`
    pub const ROOT: Self = Self("");
    /// Well-known node name `/aliases`
    pub const ALIASES: Self = Self("aliases");
    /// Well-known node name `/memory`
    pub const MEMORY: Self = Self("memory");
    /// Well-known node name `/reserved-memory`
    pub const RESERVED_MEMORY: Self = Self("reserved-memory");
    /// Well-known node name `/chosen`
    pub const CHOSEN: Self = Self("chosen");
    /// Well-known node name `/cpus`
    pub const CPUS: Self = Self("cpus");
    /// Well-known node name `/cpus/cpu*`
    pub const CPU: Self = Self("cpu");

    pub fn without_unit(&self) -> Self {
        if let Some(len) = self.0.find("@") {
            Self(unsafe { self.0.get_unchecked(..len) })
        } else {
            Self(&self.0)
        }
    }

    pub fn unit(&self) -> Option<&str> {
        let Some(len) = self.0.find("@") else {
            return None;
        };
        self.0.get((len.wrapping_add(1))..)
    }
}

impl<'a> NodeName<'a> {
    #[inline]
    pub const fn new(name: &'a str) -> Self {
        Self(name)
    }

    #[inline]
    pub const fn as_str(&'a self) -> &'a str {
        if self.0.len() == 0 { "/" } else { self.0 }
    }
}

impl fmt::Display for NodeName<'_> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PropName<'a>(pub &'a str);

impl PropName<'_> {
    /// Well-known property name `#address-cells`, <u32>
    pub const ADDRESS_CELLS: Self = Self("#address-cells");
    /// Well-known property name `bootargs`, <string>
    pub const BOOTARGS: Self = Self("bootargs");
    /// `#clock-cells`
    pub const CLOCK_CELLS: Self = Self("#clock-cells");
    /// Well-known property name `clock-frequency`, <prop-encoded-array>
    pub const CLOCK_FREQUENCY: Self = Self("clock-frequency");
    /// Well-known property name `compatible`, <string-list>
    pub const COMPATIBLE: Self = Self("compatible");
    /// Well-known property name `device_type`, <string> (deprecated)
    pub const DEVICE_TYPE: Self = Self("device_type");
    /// Well-known property name `dma-coherent`, <empty>
    pub const DMA_COHERENT: Self = Self("dma-coherent");
    /// Well-known property name `dma-ranges`, <prop-encoded-array>
    pub const DMA_RANGES: Self = Self("dma-ranges");
    /// Well-known property name `#interrupt-cells`, <u32>
    pub const INTERRUPT_CELLS: Self = Self("#interrupt-cells");
    /// Well-known property name `interrupt-controller`, <empty>
    pub const INTERRUPT_CONTROLLER: Self = Self("interrupt-controller");
    /// Well-known property name `interrupt-map`, <prop-encoded-array>
    pub const INTERRUPT_MAP: Self = Self("interrupt-map");
    /// Well-known property name `interrupt-map-mask`, <prop-encoded-array>
    pub const INTERRUPT_MAP_MASK: Self = Self("interrupt-map-mask");
    /// Well-known property name `interrupts`, <prop-encoded-array>
    pub const INTERRUPTS: Self = Self("interrupts");
    /// Well-known property name `interrupt-parent`, <phandle>
    pub const INTERRUPT_PARENT: Self = Self("interrupt-parent");
    /// Well-known property name `interrupts-extended`, <phandle> <prop-encoded-array>
    pub const INTERRUPTS_EXTENDED: Self = Self("interrupts-extended");
    /// Well-known property name `model`, <string>
    pub const MODEL: Self = Self("model");
    /// Well-known property name `name`, <string> (deprecated)
    pub const NAME: Self = Self("name");
    /// Well-known property name `no-map`, <empty>
    pub const NO_MAP: Self = Self("no-map");
    /// Well-known property name `phandle`, <u32>
    pub const PHANDLE: Self = Self("phandle");
    /// Well-known property name `ranges`, <prop-encoded-array>
    pub const RANGES: Self = Self("ranges");
    /// Well-known property name `reg`, <prop-encoded-array>
    pub const REG: Self = Self("reg");
    /// Well-known property name `reusable`, <empty>
    pub const REUSABLE: Self = Self("reusable");
    /// Well-known property name `serial-number`, <string>
    pub const SERIAL_NUMBER: Self = Self("serial-number");
    /// Well-known property name `#size-cells`, <u32>
    pub const SIZE_CELLS: Self = Self("#size-cells");
    /// Well-known property name `status`, <string>
    pub const STATUS: Self = Self("status");
    /// Well-known property name `stdout-path`, <string>
    pub const STDOUT_PATH: Self = Self("stdout-path");
    /// Well-known property name `stdin-path`, <string>
    pub const STDIN_PATH: Self = Self("stdin-path");
    /// Well-known property name `timebase-frequency`, <prop-encoded-array>
    pub const TIMEBASE_FREQUENCY: Self = Self("timebase-frequency");
    /// Well-known property name `virtual-reg`, <u32>
    pub const VIRTUAL_REG: Self = Self("virtual-reg");
}

impl<'a> PropName<'a> {
    #[inline]
    pub const fn new(name: &'a str) -> Self {
        Self(name)
    }

    #[inline]
    pub const fn as_str(&'a self) -> &'a str {
        self.0
    }
}

impl fmt::Display for PropName<'_> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

pub struct FdtChildNodes<'a> {
    tokens: FdtTokens<'a>,
    level: isize,
    address_cells: u32,
    size_cells: u32,
}

impl<'a> FdtChildNodes<'a> {
    fn new(tokens: FdtTokens<'a>, address_cells: u32, size_cells: u32) -> FdtChildNodes<'a> {
        Self {
            level: 0,
            tokens,
            address_cells,
            size_cells,
        }
    }
}

impl<'a> Iterator for FdtChildNodes<'a> {
    type Item = Node<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.level < 0 {
            return None;
        }
        while let Some(token) = self.tokens.next() {
            match token {
                Token::BeginNode(name) => {
                    self.level += 1;
                    if self.level == 1 {
                        return Some(Node::new(
                            self.tokens.fork(),
                            name,
                            self.address_cells,
                            self.size_cells,
                        ));
                    }
                }
                Token::EndNode => {
                    self.level -= 1;
                    if self.level < 0 {
                        break;
                    }
                }
                Token::Prop(_, _, _) => continue,
            }
        }
        None
    }
}

pub struct FdtProps<'a> {
    tokens: FdtTokens<'a>,
}

impl<'a> FdtProps<'a> {
    #[inline]
    fn new(tokens: FdtTokens<'a>) -> FdtProps<'a> {
        Self { tokens }
    }
}

impl<'a> Iterator for FdtProps<'a> {
    type Item = FdtProperty<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(token) = self.tokens.next() {
            match token {
                Token::BeginNode(_) => return None,
                Token::EndNode => return None,
                Token::Prop(name, ptr, len) => return Some(FdtProperty { name, ptr, len }),
            }
        }
        None
    }
}

pub struct FdtProperty<'a> {
    name: PropName<'a>,
    ptr: *const c_void,
    len: usize,
}

impl<'a> FdtProperty<'a> {
    #[inline]
    pub const fn name(&self) -> PropName<'a> {
        self.name
    }

    #[inline]
    pub unsafe fn ptr<T: Sized>(&self) -> *const T {
        unsafe { transmute(self.ptr) }
    }

    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn bytes(&self) -> &'a [u8] {
        unsafe { slice::from_raw_parts(self.ptr(), self.len()) }
    }

    #[inline]
    pub fn words(&self) -> &'a [BeU32] {
        unsafe { slice::from_raw_parts(self.ptr(), self.len().wrapping_shr(2)) }
    }

    #[inline]
    pub fn as_str(&self) -> &'a str {
        unsafe { _c_string(self.ptr(), self.len()) }
    }

    #[inline]
    pub fn as_u32(&self) -> Option<u32> {
        if self.len == 4 {
            Some(unsafe { (self.ptr as *const BeU32).read_volatile() }.to_be())
        } else {
            None
        }
    }

    #[inline]
    pub fn string_list(self) -> impl Iterator<Item = &'a str> {
        StringListIter::new(unsafe { self.ptr() }, self.len())
    }
}

struct FdtRsvMapIter<'a> {
    header: &'a Header,
    index: usize,
}

impl Iterator for FdtRsvMapIter<'_> {
    type Item = (u64, u64);

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            let ptr = self.header.reserve_map_ptr().add(self.index);
            let base = ptr.read_volatile().to_be();
            let size = ptr.add(1).read_volatile().to_be();
            if size > 0 {
                self.index += 2;
                Some((base, size))
            } else {
                None
            }
        }
    }
}

struct AddressAndSizeIter<'a> {
    iter: Iter<'a, BeU32>,
    address_cells: u32,
    size_cells: u32,
}

impl<'a> AddressAndSizeIter<'a> {
    #[inline]
    fn new(slice: &'a [BeU32], address_cells: u32, size_cells: u32) -> Self {
        let iter = slice.into_iter();
        Self {
            iter,
            address_cells,
            size_cells,
        }
    }
}

impl Iterator for AddressAndSizeIter<'_> {
    type Item = (u64, u64);

    fn next(&mut self) -> Option<Self::Item> {
        let base = fdt_get_reg_val(&mut self.iter, self.address_cells).ok()?;
        let size = fdt_get_reg_val(&mut self.iter, self.size_cells).ok()?;
        Some((base, size))
    }
}

fn _c_string<'a>(s: *const u8, max_len: usize) -> &'a str {
    unsafe {
        let len = _c_strlen(s, max_len);
        let slice = slice::from_raw_parts(s, len);
        str::from_utf8_unchecked(slice)
    }
}

fn _c_strlen(s: *const u8, max_len: usize) -> usize {
    unsafe {
        let mut len = 0;
        while len < max_len && s.add(len).read_volatile() != 0 {
            len += 1;
        }
        len
    }
}

struct StringListIter<'a> {
    base: *const u8,
    max_len: usize,
    cursor: usize,
    _phantom: PhantomData<&'a ()>,
}

impl<'a> StringListIter<'a> {
    #[inline]
    pub const fn new(base: *const u8, max_len: usize) -> StringListIter<'a> {
        Self {
            base,
            max_len,
            cursor: 0,
            _phantom: PhantomData,
        }
    }
}

impl<'a> Iterator for StringListIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor < self.max_len {
            let ptr = unsafe { self.base.add(self.cursor) };
            let len = _c_strlen(ptr, self.max_len - self.cursor);
            let s = _c_string(ptr, len);
            self.cursor += len + 1;
            Some(s)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Okay,
    Disabled,
    Reserved,
    Fail,
    FailSss,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RangeTriple {
    pub child: u64,
    pub parent: u64,
    pub len: u64,
}

struct RangeTripleIter<'a> {
    iter: Iter<'a, BeU32>,
    child_address_cells: u32,
    parent_address_cells: u32,
    size_cells: u32,
}

impl Iterator for RangeTripleIter<'_> {
    type Item = RangeTriple;

    fn next(&mut self) -> Option<Self::Item> {
        let child = fdt_get_reg_val(&mut self.iter, self.child_address_cells).ok()?;
        let parent = fdt_get_reg_val(&mut self.iter, self.parent_address_cells).ok()?;
        let len = fdt_get_reg_val(&mut self.iter, self.size_cells).ok()?;
        Some(RangeTriple { child, parent, len })
    }
}

fn fdt_get_reg_val(iter: &mut Iter<BeU32>, cell_size: u32) -> Result<u64, ()> {
    match cell_size {
        0 => Ok(0),
        1 => Ok(iter.next().ok_or(())?.to_be() as u64),
        2 => {
            let hi = iter.next().ok_or(())?.to_be() as u64;
            let lo = iter.next().ok_or(())?.to_be() as u64;
            Ok((hi << 32) + lo)
        }
        _ => Err(()),
    }
}
