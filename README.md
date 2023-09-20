#  moc3-rs

moc3-rs is an parser and renderer for the moc3 file format, commonly used
in 2D animation and VTuber applications. This code is uses specifications from 
[OpenL2D's documentation](https://github.com/OpenL2D/moc3ingbird), and is a
ground-up reimplementation of the core.

## Status

Currently, the implementation is targetting moc3 version 4.2, and is most of
the way there. It is currently missing part support as well as art mesh masks,
double-sided behavior, blendmodes, and screen and multiply colors. The
implementation is also deficient in performance, and the eventual goal
is to fully optimize the implementation and introduce caching and
incrementalization.

## Goal

The eventual goal of the project is to fully parse and render moc3 files, including
any future updates to the file format, as well as the auxillary files indicating
motion and physics and so on.

It is also a goal to provide a wide variety of renderers, such as OpenGL, game engine
integrations, or Vulkan, in addition to the current wgpu renderer. As stated above,
performance is also a key goal, with smart caching and incrementalization used
to avoid unnecessary work. Parallelism has also been considered, but is unlikely to
yield significant speedups.

## The moc3 format

This repository also aims to house more information about the moc3 format, but that
part is currently still on the drawing board. moc3 is a section-offset format, while
a large central table providing offsets to more tables (with offsets to yet more tables).

Parts of the moc3 format are as of yet unknown to me, but it is likely that many stem
from the limitations of the original Cubism 3 format and loader. I hope the deformers
folder sheds light on any algorithms or math used to implement the deformations and
transformations supported in the format.
