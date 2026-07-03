//! The three top-level components of the app: loading files, picking a frame
//! out of a video, and measuring an angle on the selected frame.

mod angle_editor;
mod file_loader;
mod frame_selector;

pub use angle_editor::AngleEditor;
pub use file_loader::FileLoader;
pub use frame_selector::FrameSelector;
