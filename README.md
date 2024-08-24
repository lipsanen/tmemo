I like writing my notes with Obsidian and spaced repetition. I made this little TUI program because I didn't like the alternatives. Initialize your Obsidian vault (or other directory with .md files) as a tmemo deck with
```
tmemo init
```
And then open the program with
```
tmemo
```
You can add flash cards into your .md files like this
```
front of the card :: back of the card
```
Or multiline cards like this
```
:::
front of the card
:::
back of the card
:::
```
When tmemo starts it will automatically parse all the new flashcards from the current working directory and subdirectories. The deck is saved into tmemodeck.json. It can therefore be easily version controlled and diffs are human readable. Card scheduling is done with FSRS v4.