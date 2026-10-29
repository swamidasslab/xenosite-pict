"""xpict objects in DataFrame cells draw under ``df.style`` (pandas uses ``str``)."""

import pytest

pd = pytest.importorskip("pandas")
pytest.importorskip("jinja2")

import xpict  # noqa: E402


def test_style_shows_rendered_and_mol_cells_as_images():
    df = pd.DataFrame(
        {
            "name": ["ethanol", "phenol"],
            "drawn": [xpict.render(xpict.mol("CCO")), None],
            "mol": [xpict.mol("CCO"), xpict.mol("c1ccccc1O")],
        }
    )
    html = df.style.to_html()
    assert html.count("<svg") == 3
    assert "ethanol" in html


def test_repr_stays_readable():
    m = xpict.mol("CCO")
    assert "<svg" not in repr(m)
    assert "<svg" in str(m)
