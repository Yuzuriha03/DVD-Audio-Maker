use dvda_bridges::author_loader::{
    dvda_image_command, dvda_image_write_y4m, dvda_menu_navigation, dvda_menu_subpictures,
};
use std::{ffi::CString, path::PathBuf};
fn main() {
    let arguments: Vec<_> = std::env::args().collect();
    assert_eq!(
        arguments.len(),
        4,
        "image runtime directory, menu runtime directory, fixture directory"
    );
    let directory = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();
    let image = directory.join("dvda-image.dll");
    assert!(
        !image.exists(),
        "run this acceptance example in a fresh directory; it must not replace an existing runtime"
    );
    unsafe {
        assert_eq!(dvda_image_command(c"identify missing.png".as_ptr()), -1);
    }
    for name in ["dvda-image.dll", "policy.xml", "colors.xml"] {
        std::fs::copy(
            PathBuf::from(&arguments[1]).join(name),
            directory.join(name),
        )
        .unwrap();
    }
    for name in ["dvda-menu-spu.dll", "dvda-menu-nav.dll"] {
        std::fs::copy(
            PathBuf::from(&arguments[2]).join(name),
            directory.join(name),
        )
        .unwrap();
    }
    let fixture = std::fs::canonicalize(&arguments[3]).unwrap();
    let text =
        |path: PathBuf| CString::new(path.to_string_lossy().trim_start_matches("\\\\?\\")).unwrap();
    let source = text(fixture.join("图像.png"));
    let y4m = text(directory.join("loader.y4m"));
    let spu = text(directory.join("loader.mpg"));
    let xml = text(fixture.join("菜单.xml"));
    let input = text(fixture.join("背景.mpg"));
    let navigation = text(fixture.join("导航.xml"));
    let output = text(directory.join("loader-nav"));
    let command = CString::new(format!(
        "identify -format \"%w %h\" \"{}\"",
        source.to_string_lossy()
    ))
    .unwrap();
    unsafe {
        assert_eq!(
            dvda_image_command(command.as_ptr()),
            0,
            "failed image load must retry"
        );
        assert_eq!(
            dvda_image_write_y4m(
                source.as_ptr(),
                y4m.as_ptr(),
                c"25".as_ptr(),
                c"4:3".as_ptr()
            ),
            0
        );
        assert_eq!(
            dvda_menu_subpictures(xml.as_ptr(), input.as_ptr(), spu.as_ptr()),
            0
        );
        assert_eq!(
            dvda_menu_navigation(navigation.as_ptr(), output.as_ptr()),
            0
        );
        assert_eq!(dvda_image_command(c"unsupported x".as_ptr()), -1);
        assert_eq!(
            dvda_menu_subpictures(c"missing.xml".as_ptr(), input.as_ptr(), spu.as_ptr()),
            1
        );
    }
    assert!(directory.join("loader.y4m").metadata().unwrap().len() > 100);
    println!(
        "PASS loader missing-runtime retry, image command/Y4M, RGBA callback via SPU, navigation and failures"
    );
}
