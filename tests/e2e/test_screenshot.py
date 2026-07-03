import os
from pathlib import Path

import pytest
from playwright.sync_api import Browser, Page, expect

# Regenerates docs/screenshot.jpg for the README. It loads the demo image
# into the app with the angle points placed as in flexibility-training
# practice, meaning a hip vertex, a floor-line ray towards the heel and a
# torso ray to the shoulder. This is not part of the test suite. Run it with
#
#   SCREENSHOT=1 pytest tests/e2e/test_screenshot.py

ROOT = Path(__file__).parents[2]
DEMO = Path(__file__).parent / "fixtures" / "pancake.jpg"

pytestmark = pytest.mark.skipif(
    not os.environ.get("SCREENSHOT"), reason="only runs with SCREENSHOT=1"
)


def drag(page: Page, source: tuple[float, float], target: tuple[float, float]):
    page.mouse.move(*source)
    page.mouse.down()
    # Several steps so the app's pointer-move handling kicks in.
    page.mouse.move(*target, steps=8)
    page.mouse.up()


def test_generate_readme_screenshot(browser: Browser, base_url: str):
    context = browser.new_context(
        base_url=base_url,
        viewport={"width": 1100, "height": 870},
        device_scale_factor=2,
    )
    page = context.new_page()
    page.goto("/angulito/")
    page.set_input_files("#file-input", DEMO)
    expect(page.locator("#editor-img")).to_be_visible()

    box = page.locator("#editor-img").bounding_box()
    assert box, "editor image has no bounding box"

    def at(nx: float, ny: float) -> tuple[float, float]:
        return (box["x"] + nx * box["width"], box["y"] + ny * box["height"])

    # Defaults: a=(0.25,0.75), vertex=(0.5,0.35), c=(0.75,0.75).
    # Targets: a=floor line, vertex=hip, c=upper back.
    drag(page, at(0.25, 0.75), at(0.53, 0.63))
    drag(page, at(0.5, 0.35), at(0.73, 0.63))
    drag(page, at(0.75, 0.75), at(0.58, 0.25))

    page.screenshot(path=ROOT / "docs" / "screenshot.jpg", type="jpeg", quality=90)
    context.close()
