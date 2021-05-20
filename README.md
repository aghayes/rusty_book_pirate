The IRC channel banned all RBP[5#s] names and seems unhappy with the client. That being noted I will probably push out one more release to fix major errors and then cease work on the project.

# rusty_book_pirate
An application for downloading public domain ebooks from irc. Despite the name it is not to be used for illegal activities and I am in no way responsible for any such use.

# How to Use
Download the binary for your system (rusty_book_pirate.exe for windows rusty_book_pirate for linux) or compile them using the rust source code.

Please note that the program can be very slow especially when searching or downloading books. The search and download bots in IRC can be quiet slow and there is nothing I can do about that. If the program doesn't time out within 60 seconds of requesting something then it has recieved a response and it just waiting in line.

Run the binary and select a server with your arrow keys (the top one is a good choice) and hit enter. Wait for it to connect to the irc server and channel. Then type in what you want to search for, an author, a series, a title, whatever; then hit enter. Wait for it to query the irc channel and download the search results. Select a book using the arrow keys and then hit enter. Now either leave the download directory the same (this will download the book to the same folder you have the program in) or you can hit backspace twice and type in your own local or absolute path. If the directory you select doesn't exist it will be created. Now wait for your book to download. Don't close the program until the screen saying your book has been downloaded pops up. Then you can either download another book from the same search, enter a new search, or quit the program.

# License stuff:
Everything used except for the IRC crate is MIT licensed, and this code is MIT licensed as well. The IRC source code is available here: https://github.com/aatxe/irc
