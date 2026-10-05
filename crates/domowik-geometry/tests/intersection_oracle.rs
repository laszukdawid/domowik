use domowik_format::Point;
use domowik_geometry::arrangement::intersection_point;

// 44 cases. Oracle: Python fractions.Fraction at arbitrary precision,
// correctly rounded. An independent implementation, not this one re-run.
#[rustfmt::skip]
const CASES: &[[u64; 10]] = &[
    [0xc12ab96e5248aa88, 0xc100c22496ebb8b8, 0xc12739d59e21ce66, 0x410db28d88ae08e8, 0xc12ab96e522fb9b0, 0xc100c2249709c5d5, 0xc12739d59e3abf3e, 0x410db28d88cc1605, 0xc128f9a1f8353c77, 0x40e9e0d1e384a060],
    [0x41187fd1c9c02b4c, 0x412e286351ef2fe8, 0x410825812afb7a38, 0x412444af8340ce7c, 0x41187fd1c995b1b8, 0x412e286351fc878d, 0x410825812b506d60, 0x412444af833376d7, 0x411249492f9ef434, 0x412936896a97ff32],
    [0x41270ff71417d2b4, 0xc12988eefe758aea, 0x4117cc003286c364, 0xc0d91fe5f6d8ea40, 0x41270ff7144cf906, 0xc12988eefe5d917b, 0x4117cc00321c76c0, 0xc0d91fe5f9d81820, 0x41217afb96ad9a33, 0xc11a51ee2e2c523c],
    [0x40fd35ae8a4320c0, 0x40ff2a0e03e02260, 0x41134bb674f1103c, 0xc12a9a169f701a00, 0x40fd35ae88372f94, 0x40ff2a0e03791cc4, 0x41134bb675740c87, 0xc12a9a169f63394c, 0x410a99221549dbd2, 0xc116b4d4d94fc520],
    [0x4122e40185219bd6, 0x412de4ad1030c594, 0xc0b2e5528df27500, 0xc12bc9eb40e01df3, 0x4122e40154be7da8, 0x412de4ad20293578, 0xc0b2e53a5c635e00, 0xc12bc9eb50d88dd8, 0x4112be36e014b43c, 0x40e0d60e7bf08f29],
    [0x412dea72ad0a12f6, 0x4129213d16099684, 0x40d304b1accd6660, 0xc0e91441af1a0dc0, 0x412dea7296a4e02e, 0x4129213d2ea2545e, 0x40d304b47973bf50, 0xc0e9144338a5eb58, 0x411e82983a20ccbd, 0x41178ff8facf661d],
    [0xc0fd0779389ab078, 0xc1256214f9aaaa6e, 0xc126be24f64b3481, 0xc128088634609608, 0xc0fd07794a63a3e4, 0xc1256214e9a1f18d, 0xc126be24f4121614, 0xc128088644694ee9, 0xc11a5f141d68eb7e, 0xc126b54d9706586c],
    [0xc11ecefbdd232686, 0xc124a504241397e4, 0xc11621ea752e7352, 0xc12651800d9cd076, 0xc11ecefbdff206fe, 0xc124a50427b72d41, 0xc11621ea725f92da, 0xc126518009f93b19, 0xc11a78732928ccec, 0xc1257b4218d8342d],
    [0xc1280966fb44702e, 0xc12462b9ce4a3734, 0xc120d7951d45e470, 0x4129d9053bd7cb1a, 0xc1280957d4ed4f1e, 0xc12462bc29cc4b3e, 0xc120d7a4439d0580, 0x4129d9079759df24, 0xc124707e0c452a4f, 0x40f5d92db6364f98],
    [0xc0f796a74fe0a6a8, 0xc11b9cacc8ba78f6, 0xc12b4080eea71c6f, 0xc115338fadcf9a5e, 0xc0f7969ee8eb2654, 0xc11b9c9cdb4a7166, 0xc12b4081fb85cc7a, 0xc115339f9b3fa1ee, 0xc11e3355d8a317f3, 0xc118681e3b450d01],
    [0x4117374051a445f8, 0xc1204ad8a48e420a, 0xc0f28cd684620060, 0xc125558af84241b6, 0x4117373d03c253ed, 0xc1204ad4144d9978, 0xc0f28cc94cda3834, 0xc125558f8882ea48, 0x4102940ab08bc5e0, 0xc122d031ce6841e0],
    [0x412a6ba0c4f53bae, 0x412098f7760c009c, 0xc129e78c9014370d, 0x41155f4f4b9c7c98, 0x412a6b9ed50dd28d, 0x412099089b604380, 0xc129e78aa02ccded, 0x41155f2d00f3f6cf, 0x40c082869c333cd9, 0x411b489f1bda4fc5],
    [0xc103fd861f345304, 0xc12940fa37c00ca4, 0xc122d70d6b464144, 0xc110e651149f8300, 0xc103f4ec7f826ffb, 0xc1293f345a4347a4, 0xc122d933d332ba06, 0xc110e9dccf990d00, 0xc117d66ef3135680, 0xc120da116107e6c8],
    [0xc128fe65bc49989d, 0xc121b5aa6ce0523c, 0xc10380972b6bf628, 0xc12bdd8971ad8265, 0xc128ffb230785206, 0xc121b83dd2d441d0, 0xc1037b655ab11082, 0xc12bdaf60bb992d0, 0xc11dde8b87249a0d, 0xc126c999ef46e955],
    [0x411e2df0b9b20698, 0x412d5d4e288d15a4, 0x40ff24da2a4753f0, 0x41212a5dbbe50102, 0x411e2ad0e2dd26ae, 0x412d5ebcda5f7d71, 0x40ff3159859ad39a, 0x412128ef0a129935, 0x4112fb93a221ee47, 0x412743d5f2390b97],
    [0x410dd2f5be617f98, 0xc121731300772970, 0x40b2ec9714fdd880, 0xc11e60746b828768, 0x410dd41da25ef262, 0xc12172237d9d7a9c, 0x40b2c79a954f7f50, 0xc11e62537135e511, 0x40fe6a5a770974f1, 0xc12051a69b1c36d3],
    [0x4120ea3391693484, 0x4115fe63913a89b8, 0xc1103446d6e9ac48, 0x41125b1fa92e7970, 0x4120c2f6093224a8, 0x41187b6e099ee9a4, 0xc10fcb978cf71920, 0x410fbc2a61943309, 0x4101a0204be8bcbb, 0x41142cc19d348194],
    [0x40ed888f67a77be0, 0x4124dbe5947760f2, 0xc10eab4be6be8efc, 0x4125e149abb8f7e0, 0x40edf7fdcc49be04, 0x4125561d663c8f01, 0xc10ec7277fe71f85, 0x41256711d9f3c9d1, 0xc0f749280cd4b004, 0x41255e97a0182c69],
    [0xc0fd8f3b811ea100, 0x412e60c549e2e35c, 0x4113a827ff43f1fc, 0x4104586e73d97c78, 0xc103b1ccf7b7a6d9, 0x412da3c9673ed234, 0x41161d3f9ad81d28, 0x41074c5dfe69c116, 0x40f888b23df89381, 0x4121bb70736ca13b],
    [0xc125f56823927a92, 0xc12a7dee9a033fb7, 0xc11f460b44ab2002, 0x412a907562d49e66, 0xc1234b603fbd2f80, 0xc12aacc9ee1cea38, 0xc1224d0d862adb14, 0x412abf50b6ee48e6, 0xc122cc36e2f4054a, 0x409286c8d15ea53e],
    [0x410ee58c5c1810a0, 0xc0f362cccc2086e8, 0xc12170c4ea5d65a8, 0xc124cd630a467482, 0xc129986b682a430a, 0x412493fac36ce28c, 0x41190812fc8dde38, 0xc12b6f5593cfd810, 0xc0ecf6ef0adbcbb6, 0xc112c60332e1ed86],
    [0x40a101ab6ed47600, 0xc12057234ebad9d6, 0x411a7cc6ecc90acc, 0x412c1ccf2320d5fc, 0x412478a0c5cb468a, 0x412e329d47c94d66, 0xc10f385ce9766460, 0x41146f8d6469eea8, 0x411838a56b05b9e4, 0x41284a497040844e],
    [0x412771d35730502a, 0x4105be80300f22a0, 0xc1297943f109cbe8, 0x411d01546aa0eef4, 0x4111ae84600bb558, 0xc11897549bf95d2e, 0x4118ff6d4d67b57c, 0x411ff15f77726e88, 0x4116d8fe577b5139, 0x410ea7eb46d184e8],
    [0xc10ac36a805e70e0, 0x411bf32cc4572ab4, 0xc0fbecc5e692cfa0, 0xc121557478f73864, 0x40f0a9c369090520, 0xc116de788bb5a0e6, 0xc1025d745c656fc8, 0xc1199664a6e6366c, 0xc0ffbdc2b680a424, 0xc119555e6d3f0dfb],
    [0xc0f09df9d28cf200, 0xc126a67ee1eaeb08, 0xc12b78da8f81a7fc, 0xc0fe5707f86fac88, 0xc110a86eb64b3f4e, 0xc0e0d1eb468cf570, 0xc11194a952a11c82, 0xc129d2f055f05a1c, 0xc11147ec1d74a100, 0xc121c6cbb6eeb460],
    [0xc11bdbd496ab6a68, 0x410fa3884d11a450, 0x4125a96179c03e3c, 0x4126c5ceee62854c, 0xc1245d6d8f695fd2, 0xc1006ec0d844b744, 0x40eedb5a098f5100, 0x412bb71e551ffca6, 0xc116406e181c1ac9, 0x411229326cbf8d47],
    [0xc123c76b31e56db6, 0xc121340e366c777b, 0x4127421f0eb8976a, 0x4121b937b998bc50, 0x40bd556d67a74300, 0xc119b00ece648304, 0xc12cf7a2d2b694b6, 0xc0dfc580ad082500, 0xc1130e00deb76eab, 0xc111c46cd9e1b992],
    [0xc1113dba2f2135ce, 0x41294e92804e2a98, 0x411724154424543c, 0x4126f6849ea95cf4, 0xc106dfa4e67c4128, 0x412dd5b358de3140, 0x412495d98a00d580, 0xc118705002e83782, 0xc0f37fc61509bfb3, 0x412896d0c8e2709a],
    [0xc0ed07c72eb7d140, 0xc1215720c7e5df72, 0x412312313739760a, 0xc109ec8cc49849dc, 0x4127e89593d9ab34, 0xc11b3801c3ed0524, 0xc128946187c966f1, 0x40d6283a88f9fac0, 0x4118261ede72f16c, 0xc1143cdcf53f8617],
    [0x4120bbbce06f2410, 0xc0f23a8ce74c5eb8, 0xc1026f721cd98738, 0xc117e22267d53db4, 0xc12364c110b21a84, 0x412b6dbd60b64f2e, 0x410abd15aac01668, 0xc113ebbf36204d92, 0x410418f541e62e71, 0xc10e51f8f53a6274],
    [0x412503920bf1759a, 0x40f61c4f66186620, 0xc0e6fd01b3859980, 0x4102491640d3ac28, 0xc0e45a856009bb80, 0x4116926c076c1490, 0x410c8d2afb291450, 0xc128eb4042471bce, 0x40c482d1a0bdc0c5, 0x4101b8362569da65],
    [0xc119cde37d730a9c, 0xc0d88794e272f820, 0x4115cb03e1c8b62c, 0xc11259e582031a3a, 0xc120f00a5bec0ce8, 0xc11b5cedfdb23810, 0x4123bc3773c82732, 0x411544d062794d58, 0xc0f69fd188ee21a9, 0xc1014df6c3590dd4],
    [0xc115dc07667197fe, 0xc123d2e0cfa28d42, 0x41281ce0a4271f06, 0x4119fcab50999e44, 0xc0ab7ea75567ae00, 0xc11ed044e77321ab, 0x41289cbd93c9c0b4, 0x4120748cb7a1a048, 0x41201df889ecd5a8, 0x410605a6b71894d2],
    [0x4125d2069a302b2c, 0x41188f2a01272ed8, 0xc11ee5a6313488f8, 0xc11472f44b91d6f6, 0xc0b3ba029aae0b00, 0x40fc75cc388bd460, 0x40d035e9c785e6c0, 0xc12c53516c3cce79, 0xc09f42ec05b11482, 0xc0dddbdbf6cb7773],
    [0xc126ae3f2ce54c3c, 0x41206921b332eda4, 0xc10dc35a06a3539c, 0xc12e346e21a9caff, 0xc110a18d5ccf994e, 0xc12dbf27b82d6b09, 0x41241f867701db52, 0xc1269ee43de29a04, 0xc10e94ecd69cb2b7, 0xc12d942b966e1328],
    [0x4116d5a3c852be88, 0xc124a42c588198ba, 0x40fff7fa4c067060, 0x4114569249e6a588, 0xc0cb61afe2503800, 0x41127612473420f8, 0x411465c9340995fc, 0xc11c9753d076ed18, 0x411311b8888dfe18, 0xc119a6763c64ac3c],
    [0xc119c54a0cb73710, 0x4127135ef280e0a4, 0x41211aa0048dfabc, 0xc11be49d1b833af4, 0x410ca664b42fac08, 0x41138001aa0a05d0, 0x40e9518672eaea80, 0xc0fa3d5714882da8, 0x40ffb2a90e619bbb, 0x40f2379b919f48ce],
    [0x4126aa77c7584e14, 0x41245356f78ec6ec, 0x4100e107b7689be8, 0xc10a6c23b6913f48, 0xc10518c1ca84f920, 0x41216f087e100f60, 0x412b67d007cfb650, 0x40f21398ed8049a0, 0x411cd7b4146383fc, 0x411093ba27bdbefc],
    [0x412b3ade70bec298, 0x412444e498d67234, 0x4110bdd249e56d20, 0xc1199b77c6f619d0, 0x411994d313bad90c, 0xc10fab2d4e638320, 0xc12c4cf7e50a0cbb, 0xc0ff6b16e5cd0910, 0x41167c5b1488d8e5, 0xc10f11487b6b9415],
    [0x4112786fe7075764, 0x411657e38370cd8c, 0xc12904449847ddd8, 0xc1179f09bd6802f2, 0xc11cbd5a417f69e4, 0xc0e5f6a6a9c75f00, 0x41231e2ffa0e0388, 0xc0ffa386cb927280, 0xc113eed208d3e9cb, 0xc0eb655df83c5852],
    [0xc11e36bcb31f43e6, 0x412443d04b9d2f40, 0x411f282a91a758cc, 0x412b33388da2a148, 0x411f6744afe49ec0, 0x412bec3d58f59658, 0xc11057bdc8ae53e4, 0xc11fe8305b2366ec, 0x411e8a8c77f567e7, 0x412b2168cc819938],
    [0xc12b26366c90ab54, 0xc12b68eda0e4775d, 0xc111b12cb07a140c, 0x412a0cec14e40656, 0x412199e1e4009616, 0xc10ac319d84956d4, 0xc128fb856abe0fe4, 0x412d449598475f62, 0xc117388883410a49, 0x4121f9ebbf53f97e],
    [0x411f3dbc5df31ee8, 0xc0e37389acd9ca60, 0xc0f35a4276c91eb0, 0xc11e633cdc194e67, 0x4122236389428afe, 0x411f99f78c72ebe4, 0x410b26c2e63440e8, 0xc127640e3a47ec5e, 0x4119067444de15d2, 0xc0fcfdd1f89821a0],
    [0xc10820a87fdc1448, 0x40f3888d22672a40, 0xc12669a6199eeceb, 0x4111f9827a928f5c, 0xc118f91012034df8, 0xc0e97eb50f44b400, 0x4107c9dd0efc58c8, 0x41174d550d63344c, 0xc10a486df8b1cac9, 0x40f54184dca81a97],
];

#[test]
fn every_crossing_matches_an_independent_rational_oracle() {
    let f = f64::from_bits;
    for case in CASES {
        let got = intersection_point(
            Point::new(f(case[0]), f(case[1])),
            Point::new(f(case[2]), f(case[3])),
            Point::new(f(case[4]), f(case[5])),
            Point::new(f(case[6]), f(case[7])),
        )
        .expect("the oracle only lists crossings that exist");
        assert_eq!(got.x.to_bits(), case[8], "x for {case:016x?}");
        assert_eq!(got.y.to_bits(), case[9], "y for {case:016x?}");
    }
    assert_eq!(CASES.len(), 44);
}

/// The crossing belongs to the two segments, not to the order they arrive in.
/// This is the property the topology contract rests on: the same intersection
/// approached from either edge must be ONE vertex, and exact evaluation makes
/// that true by construction rather than by welding two near answers together.
#[test]
fn the_crossing_is_invariant_under_argument_order_and_reversal() {
    let f = f64::from_bits;
    for case in CASES {
        let (a, b) = (
            Point::new(f(case[0]), f(case[1])),
            Point::new(f(case[2]), f(case[3])),
        );
        let (c, d) = (
            Point::new(f(case[4]), f(case[5])),
            Point::new(f(case[6]), f(case[7])),
        );
        let base = intersection_point(a, b, c, d).expect("crosses");
        for (p, q, r, s) in [
            (b, a, c, d),
            (a, b, d, c),
            (b, a, d, c),
            (c, d, a, b),
            (d, c, b, a),
        ] {
            let got = intersection_point(p, q, r, s).expect("still crosses");
            assert_eq!(got.x.to_bits(), base.x.to_bits(), "x for {case:016x?}");
            assert_eq!(got.y.to_bits(), base.y.to_bits(), "y for {case:016x?}");
        }
    }
}

/// Scaling by a power of two is exact on every input, so the answer must scale
/// with them exactly. Only powers of two are used: any other factor would
/// round the INPUTS, and the test would then be measuring that instead.
#[test]
fn scaling_every_input_by_a_power_of_two_scales_the_crossing_exactly() {
    let f = f64::from_bits;
    for case in CASES {
        for k in [-8i32, -1, 1, 8] {
            let m = 2f64.powi(k);
            let scaled: Vec<f64> = case[..8].iter().map(|b| f(*b) * m).collect();
            if !scaled.iter().all(|v| v.is_finite()) {
                continue;
            }
            let got = intersection_point(
                Point::new(scaled[0], scaled[1]),
                Point::new(scaled[2], scaled[3]),
                Point::new(scaled[4], scaled[5]),
                Point::new(scaled[6], scaled[7]),
            )
            .expect("scaling cannot destroy a crossing");
            assert_eq!(got.x, f(case[8]) * m, "x at 2^{k} for {case:016x?}");
            assert_eq!(got.y, f(case[9]) * m, "y at 2^{k} for {case:016x?}");
        }
    }
}

#[test]
fn non_finite_input_yields_no_crossing_rather_than_a_non_finite_point() {
    let ok = Point::new(0.0, 0.0);
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let p = Point::new(bad, 1.0);
        let q = Point::new(1.0, bad);
        assert_eq!(intersection_point(p, ok, ok, ok), None);
        assert_eq!(intersection_point(ok, q, ok, ok), None);
        assert_eq!(intersection_point(ok, ok, p, ok), None);
        assert_eq!(intersection_point(ok, ok, ok, q), None);
    }
}

#[test]
fn parallel_and_non_meeting_segments_have_no_crossing() {
    let p = |x: f64, y: f64| Point::new(x, y);
    // Parallel, distinct.
    assert_eq!(
        intersection_point(p(0.0, 0.0), p(10.0, 0.0), p(0.0, 1.0), p(10.0, 1.0)),
        None
    );
    // Collinear: no single crossing parameter exists.
    assert_eq!(
        intersection_point(p(0.0, 0.0), p(10.0, 0.0), p(3.0, 0.0), p(7.0, 0.0)),
        None
    );
    // Lines cross, segments do not: membership is decided on exact signs.
    assert_eq!(
        intersection_point(p(0.0, 0.0), p(1.0, 0.0), p(5.0, -1.0), p(5.0, 1.0)),
        None
    );
    // Touching at an endpoint IS a crossing, and reuses that endpoint exactly.
    assert_eq!(
        intersection_point(p(0.0, 0.0), p(10.0, 0.0), p(10.0, 0.0), p(10.0, 5.0)),
        Some(p(10.0, 0.0))
    );
}
