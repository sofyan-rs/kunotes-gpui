//! The app's asset source: gpui-kit's default icons plus the extra Lucide
//! icons KuNotes uses. Only the icons listed here are embedded in the binary.

use std::borrow::Cow;

use gpui_kit::{AssetSource, Result, SharedString};

// Icons that gpui-kit's default set doesn't include. Add a name here before using it,
// otherwise the icon renders blank.
gpui_kit::assets::icon_assets!(
    ExtraIcons,
    [
        Bold,
        Close,
        CloudCheck,
        CloudOff,
        Code,
        Columns2,
        Eye,
        FolderPlus,
        GitBranch,
        Heading1,
        Heading2,
        Heading3,
        ImagePlus,
        Italic,
        Link,
        List,
        ListOrdered,
        ListTodo,
        NotebookPen,
        PenLine,
        Pin,
        Quote,
        SquareCode,
        SquarePen,
        Strikethrough,
        Table,
        Trash,
    ]
);

pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}
