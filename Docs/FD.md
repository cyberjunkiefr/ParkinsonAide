### ParkinsonAide

This project is meant to be an aid for Parkinson's patients to learn how to better speak. 

We will need a computer, a microphone connected to the computer, and for the PC we must have a Rust program which does the aids. 

We will be using a microphone of the type SEEED STUDIO XVF3800. It is a highly configurable mi-microphone, and the commands can be found in the file .\usage_xvf_host.md and there also are two  sample program that show how it can be used: .\respeaker_get_doa.py and .\xvf_host.py .

###  The program

The program ,  must  be a Rust app.

The app uses a text file containing a certain number of phrases in french

The app must be able to load, save, create and modify the text file.

When the app starts, the last used text file is loaded.

The app needs to have a GUI like this:

![GUI](D:\_WorkData\Projects\XVF3800\ParkinsonAide\pictures\GUI.jpg)

On loading the first 6 phrases of the text file  go on the fields Phrase1 to Phrase6 :

The field  Speech contains 4 buttons:

- Record button (.\icons\record.png)
- Play button (.\icons\play-button.png)
- Down  button (.\\icons\down-arrow.png)
- Up button (.\icons\up-arrow.png)

##### The Record button

The patient must click and hold this button to start recording and while holding the button the patient must read out loud the sentence in the field Phrase1. When he is  done he can release the button this stops the recording 0.5sec later and stores the recorded audio. At the same time a speech-to-text app will convert the audio to text and show it in the field Speech. The app will then compare the words in the sentence from Phrase1 with the text in the field Speech and give a score of 0% to 100%. where 0% means no words match and 100% all the words match.  The score is shown in the field Speech.

##### The  Play button

This button lets you play the last recorded audio file.

##### The Down button

This button lets you shift the sentence from text file in the Phrase fields. The shift happens on the release of the button.

example: 

Phrase1 contains sentence 6     -------> release of the button ----->    Phrase1 contains sentence 5

Phrase6 contains sentence 12                                                                  Phrase6 contains sentence 11 

The button is disabled when sentence 1 is in Phrase1   

##### The Up button

This button lets you shift the sentence from text file in the Phrase fields. The shift happens on the release of the button.

example: 

Phrase1 contains sentence 1     -------> release of the button ----->    Phrase1 contains sentence 2

Phrase6 contains sentence 6                                                                   Phrase6 contains sentence 7      

The button is disabled when last sentence of the text file is in Phrase1. If the last sentence of the

 text file is in Phrase6, Phrase5,  Phrase4,  Phrase3 or Phrase 2 then enter an empty line in Phrase6.

### General

Information about the configuration Seeed studio ReSpeaker XVF3800 microphone can be found here:

- .\Docs\xvf3800_user_guide_v3.2.1.pdf

- .\Docs\usage_xvf_host.md

- \Docs\respeaker_get_doa.py

  The API that handels the speech-to-text part must be open source and be able to handel french language.

  All text in the program must be in french

  When moving the sentences with the up and down buttons, the result of the speech-to-text will also move and text and the score will be remembered- This memory will reset at startup or loading of a new text file.   .

  

  

  ​                																																																																																																																																						



##### 