use crate::*;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

#[derive(Debug)]
pub struct HidParsedReport {
    report_ids: Vec<HidReportId>,
    primary: Option<ParsedReportApplication>,
    tagged: BTreeMap<HidReportId, ParsedReportApplication>,
}

impl HidParsedReport {
    #[inline]
    pub const fn new() -> Self {
        Self {
            report_ids: Vec::new(),
            primary: None,
            tagged: BTreeMap::new(),
        }
    }

    #[inline]
    pub fn report_ids(&self) -> &[HidReportId] {
        &self.report_ids
    }

    #[inline]
    pub fn primary_app(&self) -> Option<&ParsedReportApplication> {
        self.primary.as_ref()
    }

    #[inline]
    pub fn app_by_report_id(&self, report_id: HidReportId) -> Option<&ParsedReportApplication> {
        self.tagged.get(&report_id)
    }

    #[inline]
    pub fn applications(&self) -> impl Iterator<Item = &ParsedReportApplication> {
        self.tagged.values()
    }

    pub fn parse(report_desc: &[u8]) -> Result<HidParsedReport, usize> {
        let mut parsed_report = HidParsedReport::new();

        let mut current_app = ParsedReportApplication::empty();
        let mut collection_ctx = Vec::new();

        let mut reader = HidReporteReader::new(report_desc);
        let mut stack = Vec::new();
        let mut global = HidReportGlobalState::new();
        let mut local = HidReportLocalState::new();

        while let Some(lead_byte) = reader.next() {
            let lead_byte = HidReportLeadByte(lead_byte);
            let (tag, param) = if lead_byte.is_long_item() {
                let len = match reader.next() {
                    Some(v) => v as usize,
                    None => return Err(reader.position()),
                };
                let lead_byte = match reader.next() {
                    Some(v) => v,
                    None => return Err(reader.position()),
                };
                if reader.advance_by(len).is_err() {
                    return Err(reader.position());
                }
                let lead_byte = HidReportLeadByte(lead_byte);
                let tag = match lead_byte.item_tag() {
                    Some(v) => v,
                    None => {
                        // log!("UNKNOWN TAG {} {:02x}", reader.position(), lead_byte.0);
                        return Err(reader.position());
                    }
                };
                let param = HidReportValue::Zero;
                (tag, param)
            } else {
                let tag = match lead_byte.item_tag() {
                    Some(v) => v,
                    None => {
                        // log!("UNKNOWN TAG {} {:02x}", reader.position(), lead_byte.0);
                        return Err(reader.position());
                    }
                };
                let param = match reader.read_param(lead_byte) {
                    Some(v) => v,
                    None => return Err(reader.position()),
                };
                (tag, param)
            };

            match tag {
                HidReportItemTag::Input => {
                    let flag = HidReportMainFlag::from_bits(param.into());
                    ParsedReportMainItem::parse(
                        &mut current_app.entries,
                        tag,
                        flag,
                        &global,
                        &local,
                    );
                    local.reset();
                }
                HidReportItemTag::Output => {
                    let flag = HidReportMainFlag::from_bits(param.into());
                    ParsedReportMainItem::parse(
                        &mut current_app.entries,
                        tag,
                        flag,
                        &global,
                        &local,
                    );
                    local.reset();
                }
                HidReportItemTag::Feature => {
                    let flag = HidReportMainFlag::from_bits(param.into());
                    ParsedReportMainItem::parse(
                        &mut current_app.entries,
                        tag,
                        flag,
                        &global,
                        &local,
                    );
                    local.reset();
                }

                HidReportItemTag::Collection => {
                    let collection_type = match HidReportCollectionType::from_u8(param.into()) {
                        Some(v) => v,
                        None => todo!(),
                    };
                    collection_ctx.push(collection_type);

                    match collection_type {
                        HidReportCollectionType::Application => {
                            if collection_ctx.contains(&HidReportCollectionType::Application) {
                                match current_app.report_id {
                                    Some(report_id) => {
                                        parsed_report.tagged.insert(report_id, current_app.clone());
                                    }
                                    None => {
                                        parsed_report.primary = Some(current_app.clone());
                                    }
                                }
                            }
                            current_app.clear_stream();
                            let usage = local.usage.first().map(|v| *v).unwrap_or_default();
                            current_app.usage =
                                UsageLong::from_maybe_short(usage, global.usage_page);
                        }
                        _ => {
                            current_app
                                .entries
                                .push(ParsedReportEntry::Collection(collection_type));
                        }
                    }

                    local.reset();
                }

                HidReportItemTag::EndCollection => {
                    match collection_ctx.pop() {
                        Some(collection_type) => match collection_type {
                            HidReportCollectionType::Application => {
                                match current_app.report_id {
                                    Some(report_id) => {
                                        parsed_report.tagged.insert(report_id, current_app.clone());
                                    }
                                    None => {
                                        parsed_report.primary = Some(current_app.clone());
                                    }
                                }
                                current_app.clear();
                            }
                            _ => {
                                current_app
                                    .entries
                                    .push(ParsedReportEntry::EndCollection(collection_type));
                            }
                        },
                        None => return Err(reader.position()),
                    }
                    local.reset();
                }

                HidReportItemTag::UsagePage => {
                    global.usage_page = UsagePage::from_u16(param.into());
                }
                HidReportItemTag::LogicalMinimum => global.logical_minimum = param,
                HidReportItemTag::LogicalMaximum => global.logical_maximum = param,
                HidReportItemTag::PhysicalMinimum => global.physical_minimum = param,
                HidReportItemTag::PhysicalMaximum => global.physical_maximum = param,
                HidReportItemTag::UnitExponent => global.unit_exponent = param.into(),
                HidReportItemTag::Unit => global.unit = param.into(),
                HidReportItemTag::ReportSize => global.report_size = param.into(),

                HidReportItemTag::ReportId => {
                    if let Some(report_id) = HidReportId::new(param.into()) {
                        if !parsed_report.report_ids.contains(&report_id) {
                            parsed_report.report_ids.push(report_id);
                        }
                        current_app.report_id = Some(report_id);
                        global.report_id = Some(report_id);
                    }
                }

                HidReportItemTag::ReportCount => global.report_count = param.into(),
                HidReportItemTag::Push => stack.push(global),
                HidReportItemTag::Pop => {
                    global = match stack.pop() {
                        Some(v) => v,
                        None => return Err(reader.position()),
                    }
                }
                HidReportItemTag::Usage => local.usage.push(param.into()),
                HidReportItemTag::UsageMinimum => local.usage_minimum = param.into(),
                HidReportItemTag::UsageMaximum => local.usage_maximum = param.into(),

                // HidReportItemTag::DesignatorIndex => todo!(),
                // HidReportItemTag::DesignatorMinimum => todo!(),
                // HidReportItemTag::DesignatorMaximum => todo!(),
                // HidReportItemTag::StringIndex => todo!(),
                // HidReportItemTag::StringMinimum => todo!(),
                // HidReportItemTag::StringMaximum => todo!(),
                // HidReportItemTag::Delimiter => todo!(),
                _ => todo!(),
            }
        }

        Ok(parsed_report)
    }

    #[inline]
    pub fn initial_bit_position(&self) -> usize {
        if self.report_ids().len() > 0 { 8 } else { 0 }
    }
}

#[derive(Clone)]
pub struct ParsedReportApplication {
    report_id: Option<HidReportId>,
    usage: Option<UsageLong>,
    entries: Vec<ParsedReportEntry>,
}

impl ParsedReportApplication {
    #[inline]
    pub const fn empty() -> Self {
        Self {
            report_id: None,
            usage: None,
            entries: Vec::new(),
        }
    }

    #[inline]
    pub const fn report_id(&self) -> Option<HidReportId> {
        self.report_id
    }

    #[inline]
    pub const fn usage(&self) -> Option<UsageLong> {
        self.usage
    }

    #[inline]
    pub fn entries(&self) -> impl Iterator<Item = &ParsedReportEntry> {
        self.entries.iter()
    }

    #[inline]
    pub fn input_items(&self) -> impl Iterator<Item = &ParsedReportMainItem> {
        self.entries().flat_map(|v| match v {
            ParsedReportEntry::Input(v) => Some(v),
            _ => None,
        })
    }

    #[inline]
    pub fn output_items(&self) -> impl Iterator<Item = &ParsedReportMainItem> {
        self.entries().flat_map(|v| match v {
            ParsedReportEntry::Output(v) => Some(v),
            _ => None,
        })
    }

    #[inline]
    pub fn feature_items(&self) -> impl Iterator<Item = &ParsedReportMainItem> {
        self.entries().flat_map(|v| match v {
            ParsedReportEntry::Feature(v) => Some(v),
            _ => None,
        })
    }

    pub fn clear_stream(&mut self) {
        self.entries = Vec::new();
    }

    pub fn clear(&mut self) {
        self.report_id = None;
        self.usage = None;
        self.clear_stream();
    }

    pub fn bit_count_for_input(&self) -> usize {
        self.bit_count(|v| matches!(v, ParsedReportEntry::Input(_)))
    }

    pub fn bit_count_for_output(&self) -> usize {
        self.bit_count(|v| matches!(v, ParsedReportEntry::Output(_)))
    }

    pub fn bit_count_for_feature(&self) -> usize {
        self.bit_count(|v| matches!(v, ParsedReportEntry::Feature(_)))
    }

    pub fn bit_count<F>(&self, predicate: F) -> usize
    where
        F: FnMut(&&ParsedReportEntry) -> bool,
    {
        let acc = self
            .entries
            .iter()
            .filter(predicate)
            .fold(0, |acc, v| acc + v.bit_count());
        acc
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ParsedReportEntry {
    Input(ParsedReportMainItem),
    Output(ParsedReportMainItem),
    Feature(ParsedReportMainItem),
    Collection(HidReportCollectionType),
    EndCollection(HidReportCollectionType),
}

impl ParsedReportEntry {
    pub fn from_item(item: ParsedReportMainItem, tag: HidReportItemTag) -> Option<Self> {
        match tag {
            HidReportItemTag::Input => Some(Self::Input(item)),
            HidReportItemTag::Output => Some(Self::Output(item)),
            HidReportItemTag::Feature => Some(Self::Feature(item)),
            _ => None,
        }
    }

    pub fn bit_count(&self) -> usize {
        match self {
            ParsedReportEntry::Input(v) => v.bit_count(),
            ParsedReportEntry::Output(v) => v.bit_count(),
            ParsedReportEntry::Feature(v) => v.bit_count(),
            ParsedReportEntry::Collection(_) => 0,
            ParsedReportEntry::EndCollection(_) => 0,
        }
    }
}

#[derive(Clone, Copy)]
pub struct ParsedReportMainItem {
    flags: HidReportMainFlag,
    report_size: u8,
    report_count: u8,
    usage_min: Option<UsageLong>,
    usage_max: Option<UsageLong>,
    logical_min: u32,
    logical_max: u32,
    physical_min: u32,
    physical_max: u32,
}

impl ParsedReportMainItem {
    #[inline]
    pub const fn empty() -> Self {
        Self {
            flags: HidReportMainFlag::empty(),
            report_size: 0,
            report_count: 0,
            usage_min: None,
            usage_max: None,
            logical_min: 0,
            logical_max: 0,
            physical_min: 0,
            physical_max: 0,
        }
    }

    pub fn parse(
        vec: &mut Vec<ParsedReportEntry>,
        tag: HidReportItemTag,
        flag: HidReportMainFlag,
        global: &HidReportGlobalState,
        local: &HidReportLocalState,
    ) {
        if local.usage.len() > 0 {
            let report_count = global.report_count / local.usage.len();
            for usage in &local.usage {
                ParsedReportEntry::from_item(
                    Self::new(flag, global, local, Some(*usage), report_count),
                    tag,
                )
                .map(|v| vec.push(v));
            }
        } else {
            ParsedReportEntry::from_item(Self::new(flag, global, local, None, 0), tag)
                .map(|v| vec.push(v));
        }
    }

    #[inline]
    pub fn new(
        flag: HidReportMainFlag,
        global: &HidReportGlobalState,
        local: &HidReportLocalState,
        usage: Option<u32>,
        report_count: usize,
    ) -> Self {
        let (usage_min, usage_max) = if flag.is_const() {
            (None, None)
        } else {
            let (usage_min, usage_max) = if let Some(usage) = usage {
                (usage, 0)
            } else {
                (local.usage_minimum, local.usage_maximum)
            };
            let usage_max = UsageLong::from_maybe_short(usage_max, global.usage_page);
            let usage_min = UsageLong::from_maybe_short(usage_min, global.usage_page);
            (usage_min, usage_max)
        };

        let report_count = if report_count > 0 {
            report_count
        } else {
            global.report_count
        } as u8;
        let logical_min = if flag.contains(HidReportMainFlag::RELATIVE) {
            global.logical_minimum.as_isize() as u32
        } else {
            global.logical_minimum.as_u32()
        };
        let logical_max = if flag.contains(HidReportMainFlag::RELATIVE) {
            global.logical_maximum.as_isize() as u32
        } else {
            global.logical_maximum.as_u32()
        };
        let physical_min = if flag.contains(HidReportMainFlag::RELATIVE) {
            global.physical_minimum.as_isize() as u32
        } else {
            global.physical_minimum.as_u32()
        };
        let physical_max = if flag.contains(HidReportMainFlag::RELATIVE) {
            global.physical_maximum.as_isize() as u32
        } else {
            global.physical_maximum.as_u32()
        };
        Self {
            flags: flag,
            report_size: global.report_size as u8,
            report_count,
            usage_min,
            usage_max,
            logical_min,
            logical_max,
            physical_min,
            physical_max,
        }
    }

    #[inline]
    pub const fn flags(&self) -> HidReportMainFlag {
        self.flags
    }

    #[inline]
    pub fn is_const(&self) -> bool {
        self.flags.is_const()
    }

    #[inline]
    pub fn is_array(&self) -> bool {
        self.flags.is_array()
    }

    #[inline]
    pub fn is_variable(&self) -> bool {
        self.flags.is_variable()
    }

    #[inline]
    pub fn is_relative(&self) -> bool {
        self.flags.is_relative()
    }

    #[inline]
    pub const fn usage_min(&self) -> Option<UsageLong> {
        self.usage_min
    }

    #[inline]
    pub const fn usage_max(&self) -> Option<UsageLong> {
        self.usage_max
    }

    #[inline]
    pub fn usage_range(&self) -> Option<UsageRange> {
        let min = self.usage_min?;
        let max = self.usage_max?;
        if min == max {
            Some(UsageRange::Single(min))
        } else {
            Some(UsageRange::Range(min, max))
        }
    }

    #[inline]
    pub const fn logical_min(&self) -> u32 {
        self.logical_min
    }

    #[inline]
    pub const fn logical_max(&self) -> u32 {
        self.logical_max
    }

    #[inline]
    pub const fn report_size(&self) -> usize {
        self.report_size as usize
    }

    #[inline]
    pub const fn report_count(&self) -> usize {
        self.report_count as usize
    }

    #[inline]
    pub const fn bit_count(&self) -> usize {
        self.report_size() * self.report_count()
    }
}

pub enum UsageRange {
    Single(UsageLong),
    Range(UsageLong, UsageLong),
}

impl core::fmt::Debug for ParsedReportApplication {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let _ = writeln!(
            f,
            "application {:02x} usage {}",
            self.report_id.map(|v| v.as_u8()).unwrap_or_default(),
            self.usage.map(|v| v.0.get()).unwrap_or_default(),
        );

        for entry in &self.entries {
            let _ = writeln!(f, "{:?}", entry);
        }

        Ok(())
    }
}

impl core::fmt::Debug for ParsedReportMainItem {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{:x} size {} {}",
            self.flags.bits(),
            self.report_size,
            self.report_count,
        )?;
        if !self.flags.contains(HidReportMainFlag::CONSTANT) {
            match self.usage_range() {
                Some(UsageRange::Single(usage)) => {
                    let _ = write!(f, " usage {}", usage);
                }
                Some(UsageRange::Range(min, max)) => {
                    let _ = write!(f, " usage {}..{}", min, max);
                }
                None => {}
            }
            if self.flags.contains(HidReportMainFlag::RELATIVE) {
                let _ = write!(
                    f,
                    " log {}..{} phy {}..{}",
                    self.logical_min as i32,
                    self.logical_max as i32,
                    self.physical_min as i32,
                    self.physical_max as i32,
                );
            } else {
                let _ = write!(
                    f,
                    " log {}..{} phy {}..{}",
                    self.logical_min, self.logical_max, self.physical_min, self.physical_max
                );
            }
        }

        Ok(())
    }
}
