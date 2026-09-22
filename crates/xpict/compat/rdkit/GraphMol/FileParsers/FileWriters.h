/* Compatibility shim: rdkit-sys 0.4.x includes FileWriters.h; Ubuntu
 * librdkit-dev 202309 ships MolToMolBlock in FileParsers.h instead. */
#pragma once
#include <GraphMol/FileParsers/FileParsers.h>
#include <GraphMol/FileParsers/MolWriters.h>
