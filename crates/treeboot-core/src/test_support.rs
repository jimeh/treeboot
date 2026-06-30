use std::path::Path;

pub(crate) fn symlink_file(
    target: impl AsRef<Path>,
    link: impl AsRef<Path>,
) -> std::io::Result<()> {
    let target = target.as_ref();
    let link = link.as_ref();

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(target, link)
    }
}

pub(crate) fn symlink_dir(target: impl AsRef<Path>, link: impl AsRef<Path>) -> std::io::Result<()> {
    let target = target.as_ref();
    let link = link.as_ref();

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(target, link)
    }
}
