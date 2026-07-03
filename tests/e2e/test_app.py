import re
from pathlib import Path

from playwright.sync_api import Page, expect

FIXTURE = Path(__file__).parent / "fixtures" / "pose.png"
VIDEO_FIXTURE = Path(__file__).parent / "fixtures" / "clip.webm"

# The fixture is 320x240, so its aspect ratio is 4/3. With the default points
# a=(0.25,0.75), vertex=(0.5,0.35), c=(0.75,0.75) the aspect-corrected angle
# is 79.6°, displayed rounded as 80°.
DEFAULT_ANGLE = "80°"


def load_fixture(page: Page) -> None:
    page.goto("/angulito/")
    page.set_input_files("#file-input", FIXTURE)
    expect(page.locator(".angle-readout")).to_be_visible()


def editor_pos(page: Page):
    box = page.locator("#editor-img").bounding_box()
    assert box, "editor image has no bounding box"
    return lambda nx, ny: (box["x"] + nx * box["width"], box["y"] + ny * box["height"])


def test_fills_in_the_page_title(page: Page):
    # index.html ships an empty <title> that dx fills in from `title` under
    # [web.app] in Dioxus.toml. dx appends rather than replaces, so a title
    # written into index.html would come out as "AngulitoAngulito". Should a
    # dx upgrade change either behavior, this catches it.
    page.goto("/angulito/")
    expect(page).to_have_title("Angulito")


def test_shows_the_app_and_loads_an_image(page: Page):
    page.goto("/angulito/")
    expect(page.get_by_role("heading", name="Angulito")).to_be_visible()
    page.set_input_files("#file-input", FIXTURE)
    expect(page.locator("#editor-img")).to_be_visible()
    expect(page.locator(".angle-readout")).to_have_text(f"Angle: {DEFAULT_ANGLE}")
    expect(page.locator(".angle-label")).to_have_text(DEFAULT_ANGLE)


def test_dragging_a_handle_updates_the_angle(page: Page):
    load_fixture(page)
    at = editor_pos(page)

    # Drag the first endpoint from (0.25, 0.75) to (0.5, 0.75). The
    # aspect-corrected angle becomes 39.8°, displayed as 40°.
    page.mouse.move(*at(0.25, 0.75))
    page.mouse.down()
    page.mouse.move(*at(0.5, 0.75), steps=5)
    page.mouse.up()

    expect(page.locator(".angle-readout")).to_have_text("Angle: 40°")


def test_nudges_a_focused_handle_with_the_arrow_keys(page: Page):
    load_fixture(page)
    # Grabbing a handle with the pointer must focus it (the pointerdown
    # handler cancels the default focus behavior and refocuses explicitly).
    # The 320px wide fixture is displayed at natural size; five Shift+Arrow
    # presses move the first endpoint by 50px, from (0.25, 0.75) to
    # (0.40625, 0.75), giving an aspect-corrected angle of 57.2°.
    page.locator(".handle-hit").first.click()
    for _ in range(5):
        page.keyboard.press("Shift+ArrowRight")
    expect(page.locator(".angle-readout")).to_have_text("Angle: 57°")


def test_shows_the_loupe_only_while_dragging(page: Page):
    load_fixture(page)
    at = editor_pos(page)

    loupe = page.locator(".loupe")
    expect(loupe).not_to_be_visible()
    page.mouse.move(*at(0.25, 0.75))
    page.mouse.down()
    expect(loupe).to_be_visible()
    page.mouse.up()
    expect(loupe).not_to_be_visible()


def test_saves_the_annotated_image_with_a_descriptive_filename(page: Page):
    load_fixture(page)
    with page.expect_download() as download_info:
        page.get_by_role("button", name="Save image").click()
    assert re.fullmatch(
        r"angulito_\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}_80deg\.png",
        download_info.value.suggested_filename,
    )


def test_loads_multiple_images_as_separate_frames(page: Page):
    load_fixture(page)
    page.set_input_files("#file-input", [FIXTURE, FIXTURE])

    # The two new images are appended to the existing frame. The last one
    # loaded is selected.
    thumbs = page.locator(".frame-strip .thumb")
    expect(thumbs).to_have_count(3)
    expect(thumbs.nth(2)).to_have_class(re.compile("selected"))

    # Selecting another frame switches the editor to it.
    thumbs.nth(0).locator("img").click()
    expect(thumbs.nth(0)).to_have_class(re.compile("selected"))
    expect(page.locator(".angle-readout")).to_be_visible()

    # Deleting frames updates the strip; removing the last one closes the
    # editor and the strip.
    thumbs.nth(0).locator(".thumb-delete").click()
    expect(thumbs).to_have_count(2)
    thumbs.nth(1).locator(".thumb-delete").click()
    thumbs.nth(0).locator(".thumb-delete").click()
    expect(page.locator(".frame-strip")).not_to_be_visible()
    expect(page.locator(".angle-readout")).not_to_be_visible()


def test_captures_a_frame_from_a_video_and_closes_the_video(page: Page):
    page.goto("/angulito/")
    page.set_input_files("#file-input", VIDEO_FIXTURE)
    expect(page.locator(".frame-selector")).to_be_visible()

    # Wait until the video has decoded the current frame, then capture it.
    page.wait_for_function(
        """() => {
            const video = document.getElementById('video-el');
            return video && video.readyState >= 2;
        }"""
    )
    page.get_by_role("button", name="Capture frame").click()
    expect(page.locator(".frame-strip .thumb")).to_have_count(1)
    expect(page.locator(".angle-readout")).to_be_visible()

    # Closing the video hides the player but keeps the captured frame.
    page.locator(".video-close").click()
    expect(page.locator(".frame-selector")).not_to_be_visible()
    expect(page.locator(".frame-strip .thumb")).to_have_count(1)
    expect(page.locator(".angle-readout")).to_be_visible()


def test_opens_only_the_first_of_several_selected_videos(page: Page):
    page.goto("/angulito/")
    page.set_input_files("#file-input", [VIDEO_FIXTURE, VIDEO_FIXTURE])
    expect(page.locator(".frame-selector")).to_be_visible()
    expect(page.locator(".status-error")).to_contain_text(
        "The first of the 2 selected videos was opened"
    )


def test_rejects_an_unsupported_file_with_an_error_message(page: Page):
    page.goto("/angulito/")
    page.set_input_files(
        "#file-input",
        {"name": "notes.txt", "mimeType": "text/plain", "buffer": b"not an image"},
    )
    expect(page.locator(".status-error")).to_contain_text("not an image or video")

    # The message can be dismissed.
    page.locator(".status-dismiss").click()
    expect(page.locator(".status-error")).not_to_be_visible()
