# cwa-server
An open source, yet-to-be-named game server compatible with the Clone Wars Adventures client from 2014.

**This project is neither affiliated with nor supported by Disney or Daybreak Game Company, formerly Sony Online 
Entertainment.**

## How Do I Play?
At this time, we are prioritizing technical users until we can create a more user-friendly setup.

You'll need to install [Rust](https://www.rust-lang.org/) to build and run the server. By default, the server runs on 
port 20225.

For testing, we use Clone Wars Adventures client version 0.180.1.530619 (2014, executable SHA-256: 
`c02f3c8a1be8dc28e517a29158c956c16393eb1def4870b0813775c69a62d2dd`). We do not distribute the game client ourselves for 
legal reasons. You will have to obtain it yourself in order to play.

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
