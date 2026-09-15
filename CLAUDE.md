# Overview
This is a multiplayer demo with the goal of rapid protyping of client server MMO communication

# Process
* work back and forth with me, starting with your open questions and outline before writing the plan. 
* Only answer if you're confident. If you're unsure about anything, tell me directly do not make something up. I'd rather verify it myself than get a wrong answer.
* Never take control of the mouse or keyboard
* Test all changes and prove they work and fufill the stated goals and acceptance criteria
* Take before and after screenshots to confirm that changes were made and done correctly

# Tech Stack
* The client is Unity version 6000.6.0f1
* The client project is under ./dumb-unity-client
* The server is a modular monolith written in Rust and backed by MySQL
* Server will be under ./dumb-server
* Communication between client and server is done via a Websocket

# Performance Requirements
* Support 150 players in a single zone.

# Coding Standards
* Follow "You aren't gonna need it" (YAGNI) principle.
* Follow Clean Code standards
* Components should be loosely coupled.
* Make use of event driven designs where possible
* Make use of finite state machines to prevent entities from being in an invalid state

# Git Rules
* Never use git to add, commit, or push changes

# Workflow
* Work with me to create a requirements document under ./docs
* Create a technical design document based on the requirements doc and save it under ./docs
* Use taskmaster skill to create a task plan
* Work through the task list 1 task at a time
