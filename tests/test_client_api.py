"""Single-mol client parity with JS / Rust ``mol`` / ``render`` / ``to_svg``."""

from __future__ import annotations

from xpict import DepictSpec, Mol, MolRenderOptions, PictBase, depict, mol, render, to_svg
from xpict.contracts.depict import CircleSpec, StrokeSpec


def test_mol_constructs():
    m = mol("CCO")
    assert isinstance(m, Mol)
    assert m.source == "CCO"


def test_circle_bonds_paint_capsule():
    svg = render(
        mol("CCO"),
        MolRenderOptions(
            circle=CircleSpec(
                bonds=[(0, 1)],
                stroke=StrokeSpec(color="#c00", width=1.4),
            )
        ),
    ).to_svg()
    assert "bond-circle" in svg
    assert "#c00" in svg
    doc = render(
        {
            "type": "mol",
            "smiles": "CCO",
            "circle": [
                {"bonds": [[1, 2]], "fill": {"color": "#2a9d8f", "opacity": 0.3}},
            ],
        }
    )
    assert "bond-circle" in doc
    assert "#2a9d8f" in doc


def test_ethanol_svg():
    rendered = render(mol("CCO"))
    svg = rendered.to_svg()
    assert "<svg" in svg
    assert "viewBox=" in svg
    assert 'class="xpict-mol"' in svg
    assert "data-id=" in svg
    assert "<?xml" not in svg
    assert rendered.width > 0 and rendered.height > 0
    assert len(rendered.molecule.atoms) == 3
    assert len(rendered.molecule.bonds) == 2
    assert rendered.frame_molblock


def test_svg_passes_through_spec_id():
    svg = depict({"type": "mol", "id": "m3", "smiles": "CCO"})[0].to_svg()
    assert 'class="xpict-mol"' in svg
    assert 'data-id="m3"' in svg
    assert "<?xml" not in svg


def test_benzene_aromatic_order_not_wiped_to_single():
    """Chematic aromatic bonds are CTAB type 4 — must reach paint, not become 1."""
    rendered = render(mol("c1ccccc1"))
    orders = [float(b.order) for b in rendered.molecule.bonds]
    assert orders == [4.0] * 6
    assert "<svg" in rendered.to_svg()


def test_benzene_e2e_svg_draws_aromatic_circle():
    """E2E: aromatic SMILES must show a ring circle, not a bare single-bond hexagon."""
    svg = render(mol("c1ccccc1")).to_svg()
    assert "<circle" in svg
    assert "aromatic" in svg


def test_benzene_kekule_opt_skips_aromatic_circle():
    """``aromatic='kekule'`` invents doubles and omits the ring circle."""
    svg = render(mol("c1ccccc1"), MolRenderOptions(aromatic="kekule")).to_svg()
    assert "aromatic" not in svg


def test_debug_box_opt_draws_layout_borders():
    """``debug_box=True`` adds dashed boxes for labels / mol / scene, not bonds."""
    svg = render(mol("CC(=O)C"), MolRenderOptions(debug_box=True)).to_svg()
    assert "debug-box-label" in svg
    assert "debug-box-mol" in svg
    assert "debug-box-scene" in svg
    assert 'stroke-dasharray="4 2"' in svg or "stroke-dasharray=\"4 2\"" in svg
    # Partial opacity so stacked boxes remain distinguishable.
    assert 'opacity="0.45"' in svg
    # Default off.
    assert "debug-box" not in render(mol("CC(=O)C")).to_svg()
    # Cascades from figure opts onto display.
    html = mol("CC(=O)C").opts(debug_box=True)._repr_html_()
    assert "debug-box-mol" in html


def test_plus_chain_aligns_aromatics_onto_previous():
    """``a + b + c`` chain-aligns onto the previous mol; ring MCS stays tight."""
    import json
    import math

    from xpict.display import render_depict_mols

    fig = mol("c1ccc(O)c(C)c1") + mol("COc1ccccc1") + mol("Cc1ccccc1")
    nodes = fig.mols()
    assert nodes[1].align_to == nodes[0].id
    assert nodes[2].align_to == nodes[1].id

    rows = render_depict_mols(fig)
    assert len(rows) == 3
    # Every non-root has MCS tags onto the previous mol (chain wiring).
    for cur in rows[1:]:
        dbg = cur.molecule.align_debug
        tags = list(dbg.tags) if dbg is not None else []
        assert len(tags) >= 5, f"expected ring MCS tags, got {tags}"

    def _frame_median(prev, cur) -> float:
        tags = cur.molecule.align_debug.tags
        pref = {int(a["index"]): a for a in json.loads(prev.frame_molblock)["atoms"]}
        cref = {int(a["index"]): a for a in json.loads(cur.frame_molblock)["atoms"]}
        dists = [
            math.hypot(
                cref[int(q)]["x"] - pref[int(r)]["x"],
                cref[int(q)]["y"] - pref[int(r)]["y"],
            )
            for q, r in tags
            if int(q) in cref and int(r) in pref
        ]
        assert dists
        return sorted(dists)[len(dists) // 2]

    # Both hops: anisole ← cresol, then toluene ← anisole (not a shifted methyl→ring map).
    assert _frame_median(rows[0], rows[1]) < 0.5
    assert _frame_median(rows[1], rows[2]) < 0.5


def test_mol_label_sugar_paints_caption_in_svg():
    """``mol(..., name=)`` → ``MolNode.name``; Rust paints caption into the mol SVG."""
    from xpict.contracts.depict import MolNode, MolTextSpec
    from xpict.display import render_depict_mols

    one = mol("CCO", name="ethanol")
    root = one.to_spec().root
    assert isinstance(root, MolNode)
    assert root.name == "ethanol"

    styled = mol("CCO", label="ethanol", label_pos="above")
    assert isinstance(styled.to_node().name, MolTextSpec)
    assert styled.to_node().name.pos == "above"  # type: ignore[union-attr]

    rows = render_depict_mols(one.to_spec())
    svg = rows[0].to_svg()
    assert 'data-text="ethanol"' in svg
    assert "mol-caption" in svg

    fig = (
        mol("c1ccccc1")
        | mol("c1ccc(O)c(CC)c1", name="C2")
        + mol("COc1ccccc1", name="C3")
        + mol("Cc1ccccc1").with_label("C4", pos="above")
    )
    html = fig._repr_html_()
    # Captions are in SVGs, not a host HTML figcaption overlay.
    assert "C2" in html and "C3" in html and "C4" in html
    assert "mol-caption" in html


def test_grid_plus_chain_aligns_fourth_onto_third():
    """``a | b + c + d`` — fourth mol chain-aligns onto third (toluene ← anisole)."""
    import json
    import math

    from xpict.display import render_depict_mols

    fig = (
        mol("c1ccccc1")
        | mol("c1ccc(O)c(CC)c1") + mol("COc1ccccc1") + mol("Cc1ccccc1")
    )
    nodes = fig.mols()
    assert len(nodes) == 4
    assert nodes[2].align_to == nodes[1].id
    assert nodes[3].align_to == nodes[2].id
    rows = render_depict_mols(fig)
    prev, cur = rows[2], rows[3]
    tags = cur.molecule.align_debug.tags
    pref = {int(a["index"]): a for a in json.loads(prev.frame_molblock)["atoms"]}
    cref = {int(a["index"]): a for a in json.loads(cur.frame_molblock)["atoms"]}
    dists = [
        math.hypot(
            cref[int(q)]["x"] - pref[int(r)]["x"],
            cref[int(q)]["y"] - pref[int(r)]["y"],
        )
        for q, r in tags
        if int(q) in cref and int(r) in pref
    ]
    assert dists
    assert sorted(dists)[len(dists) // 2] < 0.5


def test_grid_opts_cascade_color_and_debug_box():
    """``.opts`` on a ``|`` grid must reach every row via Rust chrome resolve."""
    from xpict.display import render_depict_mols
    from xpict.native_bridge import resolve_depict_chrome

    fig = (
        mol("CCCCCC")
        | (mol("c1ccc(O)c(C)c1") + mol("COc1ccccc1") + mol("Cc1ccccc1"))
    ).opts(debug_box=True, color="red")
    assert fig.root.type == "grid"
    assert fig.root.opts is not None
    assert fig.root.opts.debug_box is True
    assert fig.root.opts.color == "red"

    chromes = resolve_depict_chrome(fig)
    assert len(chromes) == 4
    for c in chromes:
        assert c.get("debug_box") is True
        assert c.get("color") == "red"

    painted = render_depict_mols(fig)
    assert len(painted) == 4
    for r in painted:
        assert r.molecule.debug_box is True
        assert r.molecule.color == "red"
        svg = r.to_svg()
        assert "debug-box-mol" in svg
        assert 'stroke="red"' in svg or "stroke='red'" in svg

    html = fig._repr_html_()
    assert "debug-box-mol" in html
    assert html.count("debug-box-mol") >= 4
    assert "stroke=\"red\"" in html or "stroke='red'" in html


def test_aromatic_opts_global_and_local_resolve_and_print():
    """Group/global ``aromatic`` inherits; local mol overrides; SVG matches."""
    from xpict.display import render_depict_mols
    from xpict.spec import DepictSpec

    # Global kekule on the group → both mols print without aromatic circles.
    group = (mol("c1ccccc1", id="a") + mol("c1ccccc1", id="b")).opts(
        aromatic="kekule"
    )
    assert group.root.opts is not None
    assert group.root.opts.aromatic == "kekule"
    painted = render_depict_mols(group)
    assert len(painted) == 2
    for r in painted:
        assert r.molecule.aromatic == "kekule"
        assert "aromatic" not in r.to_svg()
    html = group._repr_html_()
    assert html.count("<svg") == 2
    assert "aromatic" not in html

    # Local flat override on mol b → circle again for that leaf only.
    children = list(group.root.children)
    children[1] = children[1].model_copy(update={"aromatic": "circle"})
    mixed = DepictSpec(group.root.model_copy(update={"children": children}))
    painted = render_depict_mols(mixed)
    assert painted[0].molecule.aromatic == "kekule"
    assert "aromatic" not in painted[0].to_svg()
    # Default ``circle`` may be omitted from JSON (serde skip_serializing_if).
    assert painted[1].molecule.aromatic in (None, "circle")
    assert "aromatic" in painted[1].to_svg()

    # Single-mol ``.opts(aromatic=...)`` resolves and prints.
    local = mol("c1ccccc1").opts(aromatic="kekule")
    assert local.root.opts is not None
    assert local.root.opts.aromatic == "kekule"
    assert "aromatic" not in local._repr_svg_()
    assert "aromatic" in mol("c1ccccc1").opts(aromatic="circle")._repr_svg_()


def test_to_svg_from_scene():
    r = mol("c1ccccc1").render()
    svg = to_svg(r.scene)
    assert "<svg" in svg


def test_align_to_reference():
    home = render(mol("c1ccccc1"))
    aligned = render(mol("Cc1ccccc1"), MolRenderOptions(align_to=home))
    assert "<svg" in aligned.to_svg()
    assert aligned.frame_molblock


def test_align_to_rendered_and_mol():
    home = render(mol("c1ccccc1"))
    a = render(mol("Oc1ccccc1"), MolRenderOptions(align_to=home))
    assert home.mol is not None
    b = render(mol("Oc1ccccc1"), MolRenderOptions(align_to=home.mol))
    assert a.width > 0 and b.width > 0


def test_mol_ipython_repr():
    m = mol("CCO")
    assert "<svg" in m._repr_svg_()
    assert "<svg" in m._repr_html_()
    r = render(m)
    assert "<svg" in r._repr_svg_()
    assert "<svg" in r._repr_html_()


def test_mol_add_chain_aligns_to_previous():
    doc = (
        mol("CCCCCC")
        + mol("c1ccc(O)cc1")
        + mol("COc1ccccc1")
        + mol("Cc1ccccc1")
    )
    assert isinstance(doc, DepictSpec)
    assert doc.root.type == "group"
    nodes = doc.mols()
    assert [n.id for n in nodes] == ["m_0", "m_1", "m_2", "m_3"]
    assert nodes[0].align_to is None
    assert nodes[1].align_to == "m_0"
    assert nodes[2].align_to == "m_1"
    assert nodes[3].align_to == "m_2"
    html = doc._repr_html_()
    assert html.count("<svg") == 4
    assert "display:flex" in html


def test_mol_id_and_explicit_align_to():
    doc = (
        mol("c1ccc(O)cc1", id="phenol")
        + mol("CCCCCC", id="hexane", align_to="phenol")
        + mol("COc1ccccc1", id="anisole")
    )
    nodes = {n.id: n for n in doc.mols()}
    assert nodes["hexane"].align_to == "phenol"
    assert nodes["anisole"].align_to == "hexane"
    assert doc._repr_html_().count("<svg") == 3


def test_align_mode_skip_does_not_stamp_debug():
    from xpict.display import render_depict_mols

    fig = DepictSpec.model_validate(
        {
            "type": "group",
            "children": [
                {"type": "mol", "id": "a", "smiles": "c1ccccc1"},
                {
                    "type": "mol",
                    "id": "b",
                    "smiles": "Cc1ccccc1",
                    "align_to": {"ref": "a", "mode": "skip"},
                },
            ],
        }
    )
    rows = render_depict_mols(fig)
    assert rows[1].molecule.align_debug is None


def test_align_mode_greedy_stamps_mcs_tags():
    from xpict.display import render_depict_mols

    fig = DepictSpec.model_validate(
        {
            "type": "group",
            "children": [
                {"type": "mol", "id": "a", "smiles": "COc1ccccc1"},
                {
                    "type": "mol",
                    "id": "b",
                    "smiles": "Cc1ccccc1",
                    "align_to": {"ref": "a", "mode": "greedy", "timeout_ms": 0},
                },
            ],
        }
    )
    rows = render_depict_mols(fig)
    dbg = rows[1].molecule.align_debug
    assert dbg is not None
    assert len(dbg.tags) >= 3


def test_rshift_builds_reaction_scheme():
    doc = mol("CCO", id="a") >> mol("CC=O", id="b")
    assert doc.root.type == "reaction_scheme"
    children = doc.root.children
    assert children[0].id == "a"
    assert children[1].type == "edge"
    assert children[1].sources == "a"
    assert children[1].targets == "b"
    assert children[1].arrow == "forward"
    assert children[2].id == "b"
    html = doc._repr_html_()
    # One composed scheme scene (Rust ELK edges), not per-mol + Unicode arrows.
    assert html.count("<svg") == 1
    assert 'class="edge' in html


def test_lshift_swaps_edge_endpoints():
    doc = mol("CCO", id="a") << mol("CC=O", id="b")
    edge = next(c for c in doc.root.children if c.type == "edge")
    assert edge.sources == "b"
    assert edge.targets == "a"
    html = doc._repr_html_()
    assert html.count("<svg") == 1
    assert 'class="edge' in html


def test_rshift_with_plus_co_reactants():
    doc = (mol("CCO", id="a") + mol("O", id="b")) >> mol("CC=O", id="c")
    edge = next(c for c in doc.root.children if c.type == "edge")
    assert edge.sources == ["a", "b"]
    assert edge.targets == "c"


def test_rshift_chains():
    doc = mol("CCO", id="a") >> mol("CC=O", id="b") >> mol("CC(=O)O", id="c")
    edges = [c for c in doc.root.children if c.type == "edge"]
    assert len(edges) == 2
    assert edges[0].sources == "a" and edges[0].targets == "b"
    assert edges[1].sources == "b" and edges[1].targets == "c"


def test_pict_base_to_spec_and_display():
    m = mol("CCO")
    r = render(m)
    doc = m + mol("CCO")
    assert isinstance(m, PictBase) and isinstance(r, PictBase) and isinstance(doc, PictBase)
    assert m.to_spec().root.type == "mol"
    assert r.to_spec().root.type == "mol"
    assert doc.to_spec() is doc
    for obj in (m, r, doc):
        assert "<svg" in obj._repr_html_()


def test_opts_returns_new_spec():
    from xpict.native_bridge import resolve_depict_chrome

    base = mol("CCO", id="a")
    styled = base.opts(color="#0b6e4f", scale=1.2, weight=1.5)
    assert isinstance(styled, DepictSpec)
    assert styled.root.opts is not None
    assert styled.root.opts.color == "#0b6e4f"
    assert styled.root.opts.scale == 1.2
    assert styled.root.opts.weight == 1.5
    assert base.to_spec().root.opts is None
    group = (mol("CCO") + mol("CC")).opts(color="#333", weight=1.25)
    assert group.root.type == "group"
    assert group.root.opts is not None
    assert group.root.opts.color == "#333"
    assert group.root.opts.weight == 1.25
    chromes = resolve_depict_chrome(group)
    assert all(c.get("weight") == 1.25 for c in chromes)
    assert "<svg" in group._repr_html_()


def test_or_builds_grid_rows():
    doc = (
        mol("CCO") + mol("CCO")
        | mol("c1ccccc1") + mol("Cc1ccccc1")
        | mol("CCO") >> mol("CC=O")
    )
    assert doc.root.type == "grid"
    assert len(doc.root.children) == 3
    assert doc.root.children[0].type == "group"
    assert doc.root.children[1].type == "group"
    assert doc.root.children[2].type == "reaction_scheme"
    html = doc._repr_html_()
    assert "flex-direction:column" in html
    # Two group rows (2+2 mols) + one composed reaction_scheme scene.
    assert html.count("<svg") == 5
