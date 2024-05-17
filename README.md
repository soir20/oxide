# Oxide
**Oxide** is an open source game server compatible with the Clone Wars Adventures client from 2014.

**This project is neither affiliated with nor supported by The Walt Disney Company or Daybreak Game Company, formerly 
Sony Online Entertainment.**

## How Do I Play?
At this time, we are prioritizing technical users until we can create a more user-friendly setup.

You'll need to install [Rust](https://www.rust-lang.org/) to build (`cargo build`) and run (`cargo run`) the server. By default, the server runs on 
port 20225.

For testing, we use Clone Wars Adventures client version 0.180.1.530619 (2014, executable SHA-256: 
`c02f3c8a1be8dc28e517a29158c956c16393eb1def4870b0813775c69a62d2dd`). 

Clone Wars Adventures is property of Daybreak Game Company, formerly Sony Online Entertainment. Hence, we do not distribute the 
original game client. You will have to obtain it yourself in order to play.

Once you have a client from the original game, you'll need to modify your `ClientConfig.ini` to point to your client 
directory as an asset delivery server:
```shell
IndirectServerAddress=file://C:/Users/youruser/your/client/folder
```

You'll also need to add a file called `manifest.crc` to your client folder 
(`C:/Users/youruser/your/client/folder/manifest.crc`) containing the CRC-32 hash (seed: `0x04C11DB7`) of 
`Assets_manifest.txt` in **decimal** format. For client version 0.180.1.530619, the CRC-32 is
```
3015070931
```

Then you'll need to run the client from the command line with a few parameters:
```shell
CloneWars.exe inifile=ClientConfig.ini Guid=1 Server=127.0.0.1:20225 Ticket=p7w9dGPBPbbm9ZG Internationalization:Locale=8 LoadingScreenId=-1 Country=US key=9+SU7Z1u0rO/N1xW3vvZ4w== LiveGamer=1 CasSessionId=3jo7PGRQ9P4LiQF0
```

## License
This server is licensed under the GNU Affero General Public License v3. In particular, please note this section of the 
license:
> The GNU Affero General Public License is designed specifically to ensure that, in such cases, the modified source code 
> becomes available to the community. It requires the operator of a network server to provide the source code of the 
> modified version running there to the users of that server. Therefore, public use of a modified version, on a publicly 
> accessible server, gives the public access to the source code of the modified version.

## Why "Oxide"?

* The server is written in Rust (ferric oxide).
* Chemically, bonding one or more oxygen atoms with another group of atoms creates an oxide. Similarly, connecting the original client and this server makes the game playable.
* It doesn't include trademarked words or phrases.
