/**
 * Align smoke: CoordGen MCS tags (not RDKit absolute overlay).
 * Run: `npx tsx src/align.smoke.ts`
 */
import { xpict } from "./api.js";

type Rendered = Awaited<ReturnType<typeof xpict.render>>;

function tagsOf(r: Rendered): Array<[number, number]> {
  const tags = r.molecule.align_debug?.tags ?? [];
  return tags as Array<[number, number]>;
}

function assertTags(query: Rendered, min: number, label: string) {
  const n = tagsOf(query).length;
  if (n < min) {
    throw new Error(`${label}: expected ≥${min} MCS tags, got ${n}`);
  }
}

{
  const tmpl = xpict.mol("Fc1ccc(Cl)cc1");
  await xpict.render(tmpl);
  const aligned = await xpict.render(xpict.mol("Fc1ccc(Br)cc1"), { align_to: tmpl });
  assertTags(aligned, 7, "F-Cl → F-Br");
}

{
  const ethyl = xpict.mol("c1ccccc1CC");
  const pentyl = xpict.mol("c1ccccc1CCCCC");
  await xpict.render(ethyl);
  assertTags(await xpict.render(pentyl, { align_to: ethyl }), 8, "pentyl→ethyl");
  await xpict.render(pentyl);
  assertTags(await xpict.render(ethyl, { align_to: pentyl }), 8, "ethyl→pentyl");
}

{
  const phenol = xpict.mol("c1ccc(O)cc1");
  const quinone = xpict.mol("O=C1C=CC(=O)C=C1");
  await xpict.render(phenol);
  assertTags(await xpict.render(quinone, { align_to: phenol }), 6, "quinone→phenol");
}

{
  const aniline = xpict.mol("Nc1ccccc1");
  await xpict.render(aniline);
  assertTags(
    await xpict.render(xpict.mol("O=C1C=CC(=N)C=C1"), { align_to: aniline }),
    6,
    "imine→aniline"
  );
}

{
  const quinone = xpict.mol("O=C1C=CC(=O)C=C1");
  await xpict.render(quinone);
  const aligned = await xpict.render(xpict.mol("C1CCCCC1CCOCCCCCC"), {
    align_to: quinone,
  });
  if (aligned.coords.length !== 15) {
    throw new Error(`aliphatic query atom count ${aligned.coords.length}`);
  }
}

{
  const ring = xpict.mol("C1CCCOC1");
  await xpict.render(ring);
  assertTags(
    await xpict.render(xpict.mol("O=CCCCCO"), { align_to: ring }),
    4,
    "chain→THP"
  );
}

{
  const tmpl = xpict.mol("c1ccc(O)cc1");
  const tR = await xpict.render(tmpl);
  const beforeFrame = tmpl.frame_molblock;
  if (!beforeFrame) throw new Error("template missing frame_molblock");

  for (const smi of ["O=C1C=CC(=O)C=C1", "c1ccccc1CC", "Fc1ccccc1O"]) {
    const aligned = await xpict.render(xpict.mol(smi), { align_to: tmpl });
    assertTags(aligned, 6, `multi ${smi}`);
    if (tmpl.frame_molblock !== beforeFrame) {
      throw new Error(`template frame_molblock changed after aligning ${smi}`);
    }
    const tAgain = await xpict.render(tmpl);
    if (tAgain.frame_molblock !== tR.frame_molblock) {
      throw new Error(`template frame changed after aligning ${smi}`);
    }
  }
}

console.log("align smoke ok");
