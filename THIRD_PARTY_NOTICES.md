# Third-party notices

wctrl is MIT licensed, see `LICENSE`. It includes code and files from the
projects below, which stay under their own licenses. Those licenses are
reproduced here because they require it.

## What comes from where

The MCDU keeps no font of its own: one has to be uploaded after every power
cycle before the screen draws anything. The code that drives the screen and
makes that upload is ported from WwDevicesDotnet, and the files it sends are
copied unchanged into `data/mcdu`.

| In wctrl | From | License |
| --- | --- | --- |
| `crates/wctrl-hid`, the MCDU grid channel | WwDevicesDotnet, ported | BSD-3-Clause |
| `crates/wctrl-config/src/mcdu_font.rs` | WwDevicesDotnet, ported | BSD-3-Clause |
| `data/displays/mcdu.json`, grid and origins | WwDevicesDotnet | BSD-3-Clause |
| `data/mcdu/font-packet-map-3x31.json` | WwDevicesDotnet, `Resources/WinctrlFontPacketMap-3x31.json`, commit `2bf28fa` | BSD-3-Clause |
| `data/mcdu/a10c-font-21x31.json` | WCtrlDcsBiosBridge, `Resources/a10c-font-21x31.json`, commit `dd8e87b` | MIT |
| `data/mcdu/ah64d-font-21x31.json` | WCtrlDcsBiosBridge, `Resources/ah64d-font-21x31.json`, commit `2d34b12` | MIT |
| `data/mcdu/ch47f-font-21x31.json` | WCtrlDcsBiosBridge, `Resources/ch47f-font-21x31.json`, commit `586cdac` | MIT |
| `data/mcdu/f14bu-font-21x31.json` | WCtrlDcsBiosBridge, `Resources/f14bu-font-21x31.json`, commit `dd8e87b` | MIT |

* WwDevicesDotnet: <https://github.com/landre-cerp/WwDevicesDotnet>, a fork of
  mcdu-dotnet in <https://github.com/vradarserver/cduhub>.
* WCtrlDcsBiosBridge: <https://github.com/landre-cerp/WCtrlDcsBiosBridge>.

The signal catalogue in `data/catalogue` is not part of wctrl. It is generated
on each machine from the DCS-BIOS installed there, and is not distributed.

## WwDevicesDotnet

```text
BSD 3-Clause License

Copyright (c) 2025, Andrew Whewell, Laurent André

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
   contributors may be used to endorse or promote products derived from
   this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

## WCtrlDcsBiosBridge

```text
MIT License

Copyright (c) 2025 landre-cerp

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
