use crate::{
    Colors,
    cache::{self, Tables},
    grid::{Cmyk4, Rgb3, RgbCmyk},
    icc::{
        Bpc, IccError, Intent, Profile, Space, Srgb, destination_black_point, source_black_point,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProofSpec {
    pub preserve_numbers: bool,
    pub simulate_black: bool,
    pub simulate_paper: bool,
    pub intent: Intent,
}

impl Default for ProofSpec {
    fn default() -> Self {
        Self {
            preserve_numbers: true,
            simulate_black: true,
            simulate_paper: false,
            intent: Intent::Relative,
        }
    }
}

impl Colors {
    pub fn proofed(
        device: &[u8],
        document_cmyk: Option<&[u8]>,
        document_rgb: Option<&[u8]>,
        spec: ProofSpec,
    ) -> Result<Self, IccError> {
        let profile = Profile::parse(device)?;
        let rgb = document_rgb
            .map(|bytes| parse_space(bytes, Space::Rgb))
            .transpose()?;
        let (tables, paper_color) = match profile.space {
            Space::Cmyk => {
                let cmyk = if !spec.preserve_numbers && profile.can_convert_from_pcs() {
                    document_cmyk
                        .map(|bytes| parse_space(bytes, Space::Cmyk))
                        .transpose()?
                } else {
                    None
                };
                let source = match (document_cmyk, &cmyk, spec.intent) {
                    (Some(bytes), Some(_), intent) if intent != Intent::Relative => {
                        Some(Profile::parse_with_intent(bytes, intent)?)
                    }
                    _ => None,
                };
                let paper = spec
                    .simulate_paper
                    .then(|| Profile::parse_with_intent(device, Intent::Absolute))
                    .transpose()?;
                cmyk_tables(
                    &profile,
                    paper.as_ref(),
                    cmyk.as_ref(),
                    source.as_ref(),
                    rgb.as_ref(),
                    spec,
                )
            }
            Space::Rgb | Space::Gray => {
                let bytes = document_cmyk.ok_or_else(|| {
                    IccError::Unsupported("proof requires a document CMYK profile".into())
                })?;
                let cmyk = parse_space(bytes, Space::Cmyk)?;
                (
                    light_device_tables(&profile, &cmyk, rgb.as_ref(), spec.preserve_numbers)?,
                    None,
                )
            }
        };
        let key = format!(
            "proof:{spec:?}|{}|{:?}|{:?}",
            cache::content_id(device),
            document_cmyk.map(cache::content_id),
            document_rgb.map(cache::content_id),
        );
        let mut colors = Self::from_tables(tables, cache::hash64(key.as_bytes()) | 1);
        colors.proof_paper = paper_color;
        Ok(colors)
    }
}

fn parse_space(bytes: &[u8], space: Space) -> Result<Profile, IccError> {
    let profile = Profile::parse(bytes)?;
    if profile.space != space {
        return Err(IccError::Unsupported(format!(
            "expected a {space:?} profile"
        )));
    }
    Ok(profile)
}

fn rgb_xyz(rgb: Option<&Profile>, srgb: Srgb, input: [f64; 3]) -> [f64; 3] {
    rgb.map_or_else(|| srgb.to_xyz(input), |profile| profile.to_xyz(&input))
}

fn rgb_black(rgb: Option<&Profile>) -> [f64; 3] {
    rgb.map_or([0.0; 3], source_black_point)
}

fn unit(rgb: [u8; 3]) -> [f64; 3] {
    rgb.map(|v| f64::from(v) / 255.0)
}

fn ink_link(output: &Profile, source: Option<&Profile>, srgb: Srgb) -> Option<Box<[u16]>> {
    if !output.can_convert_from_pcs() {
        return None;
    }
    let bpc = Bpc::new(rgb_black(source), destination_black_point(output));
    Some(
        RgbCmyk::sample_rgb(&|input| {
            output
                .from_xyz(bpc.apply(rgb_xyz(source, srgb, input)))
                .expect("checked inverse ICC table")
        })
        .data,
    )
}

// R1 and R2.
fn cmyk_tables(
    device: &Profile,
    paper: Option<&Profile>,
    document: Option<&Profile>,
    intended_source: Option<&Profile>,
    rgb: Option<&Profile>,
    spec: ProofSpec,
) -> (Tables, Option<[u8; 3]>) {
    let srgb = Srgb::new();
    let output = paper.unwrap_or(device);
    let display_bpc = if spec.simulate_black || paper.is_some() {
        Bpc::NONE
    } else {
        Bpc::new(source_black_point(device), [0.0; 3])
    };
    let display = |ink: [f64; 4]| srgb.from_xyz(display_bpc.apply(output.to_xyz(&ink)));
    let dst_black = destination_black_point(device);
    let cmyk_bpc = document.map(|p| Bpc::new(source_black_point(p), dst_black));
    let convert_ink = |ink: [f64; 4]| {
        let device_ink = match (intended_source.or(document), cmyk_bpc) {
            (Some(source), Some(bpc)) => device
                .from_xyz(bpc.apply(source.to_xyz(&ink)))
                .unwrap_or(ink),
            _ => ink,
        };
        display(device_ink)
    };
    // R7.
    let paper_display = paper.map(|p| srgb.from_xyz8(p.to_xyz(&[0.0; 4])));
    let cmyk = if paper.is_some() {
        let white = display([0.0; 4]).map(|v| (v.clamp(0.0, 1.0) * 65535.0).round() as u16);
        Cmyk4::sample(&convert_ink, [0; 4], white)
    } else {
        Cmyk4::sample_cmyk(&convert_ink)
    };

    let rgb_bpc = Bpc::new(rgb_black(rgb), dst_black);
    let rgb_to_display = Bpc::new(rgb_black(rgb), [0.0; 3]);
    let rgb_grid = Rgb3::sample(&|input| {
        let xyz = rgb_xyz(rgb, srgb, unit(input));
        match device.from_xyz(rgb_bpc.apply(xyz)) {
            Some(ink) => srgb.from_xyz8(display_bpc.apply(output.to_xyz(&ink))),
            None => srgb.from_xyz8(rgb_to_display.apply(xyz)),
        }
    });
    let ink_space = document.unwrap_or(device);
    let tables = Tables {
        cmyk: Some(cmyk.data),
        rgb: Some(rgb_grid.rgb),
        rgb_cmyk: ink_link(ink_space, rgb, srgb),
        srgb_cmyk: ink_link(ink_space, None, srgb),
        ..Tables::default()
    };
    (tables, paper_display)
}

// R3 and R4.
fn light_device_tables(
    device: &Profile,
    document: &Profile,
    rgb: Option<&Profile>,
    preserve_numbers: bool,
) -> Result<Tables, IccError> {
    let srgb = Srgb::new();
    let to_device = |xyz| match device.space {
        Space::Gray => device.from_xyz_gray(xyz).map(|v| [v, 0.0, 0.0, 0.0]),
        Space::Rgb => device
            .from_xyz(xyz)
            .or_else(|| device.from_xyz_shaper(xyz).map(|[r, g, b]| [r, g, b, 0.0])),
        Space::Cmyk => None,
    };
    if to_device([0.0; 3]).is_none() {
        return Err(IccError::Unsupported(
            "proof device has no usable PCS-to-device transform".into(),
        ));
    }
    let round_trip = |xyz| device.to_xyz(&to_device(xyz).expect("checked inverse ICC transform"));
    let dst_black = destination_black_point(device);
    let cmyk_bpc = Bpc::new(source_black_point(document), dst_black);
    let rgb_bpc = Bpc::new(rgb_black(rgb), dst_black);
    let cmyk = Cmyk4::sample_cmyk(&|ink| {
        let xyz = cmyk_bpc
            .apply(document.to_xyz(&ink))
            .map(|v| v * black_scale(ink[3]));
        srgb.from_xyz(round_trip(xyz))
    });
    let preserved_rgb = preserve_numbers && device.space == Space::Rgb;
    let rgb_grid = Rgb3::sample(&|input| {
        let input = unit(input);
        let xyz = if preserved_rgb {
            device.to_xyz(&input)
        } else {
            round_trip(rgb_bpc.apply(rgb_xyz(rgb, srgb, input)))
        };
        srgb.from_xyz8(xyz)
    });
    let plate_source = if preserved_rgb { Some(device) } else { rgb };
    Ok(Tables {
        cmyk: Some(cmyk.data),
        rgb: Some(rgb_grid.rgb),
        rgb_cmyk: ink_link(document, plate_source, srgb),
        srgb_cmyk: ink_link(document, None, srgb),
        ..Tables::default()
    })
}

fn black_scale(black: f64) -> f64 {
    const RAMP: [(f64, f64); 10] = [
        (93.0, 1.0),
        (94.0, 0.85),
        (95.0, 0.78),
        (96.0, 0.655),
        (97.0, 0.54),
        (98.0, 0.37),
        (99.0, 0.21),
        (99.5, 0.11),
        (99.9, 0.025),
        (100.0, 0.0),
    ];
    let percent = black * 100.0;
    let upper = RAMP.partition_point(|&(k, _)| k <= percent);
    match upper {
        0 => 1.0,
        n if n == RAMP.len() => 0.0,
        n => {
            let (lo, a) = RAMP[n - 1];
            let (hi, b) = RAMP[n];
            a + (b - a) * (percent - lo) / (hi - lo)
        }
    }
}
