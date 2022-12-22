# Inside-Job
A Rust backdoor multi-session handler based on [Hoaxshell](https://github.com/t3l3machus/hoaxshell) and inspired by [Villain](https://github.com/t3l3machus/Villain).

⚠️ For example and educational purposes only! ⚠️

![Diagram from Hoaxshell](https://user-images.githubusercontent.com/75489922/197529603-1c9238ea-af14-41f7-8834-dd37ad77e809.png)

## Notice
Project in active development, not stable/polished.

## Hoaxshell
As mentioned before the project is based on Hoaxshell but includes some adjustments as to not be detected by Microsoft Defender.

## Villain
Villain is a great tool and you should check it out, this is more or less a rewrite in Rust without the peer-to-peer connectivity that Villain has to offer, although that may be added in the future.

## Goals
 - [x] Multi-session handling
 - [x] Basic CLI for managing sessions
 - [x] Windows payloads
 - [ ] Complex and rich CLI for best user experience
 - [ ] Linux payloads
 - [ ] Client interop (like Villain)