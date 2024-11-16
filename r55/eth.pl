:- use_module(library(sockets)).
:- use_module(library(iso_ext)).
:- use_module(library(lists)).

eth_return(Result) :-
	format("sending return ~w\n", [Result]),
	setup_call_cleanup(
	socket_client_open(localhost:12345, Stream, []),
	(
		format(Stream, "return ~w\n", [Result]),
		flush_output(Stream),
		read_term(Stream, Response, []),
		( Response = ok -> 
			true
		; Response = error(Msg) ->
			format("Error from server: ~w~n", [Msg]),
			fail
		; write('Unexpected response format.'), nl, fail
		)
	),
	close(Stream)
).

eth_sload(Key, Value, State) :-
	( member(Key-Value, State) ->
		true
	; fetch_from_host(Key, Value)
	).

eth_sstore(Key, Value, CurrentState, UpdatedState) :-
	exclude(Key, CurrentState, StateWithoutKey),
	UpdatedState = [Key-Value | StateWithoutKey],
	add_pending_write(Key, Value).

is_key(Key, Key-_) :- !.

init_pending_writes :-
	( bb_get(pending_writes, _) -> true ; bb_put(pending_writes, []) ).

	add_pending_write(Key, Value) :-
		bb_get(pending_writes, PendingWrites),
		bb_put(pending_writes, [Key-Value | PendingWrites]).

	pending_writes(PendingWrites) :-
		bb_get(pending_writes, PendingWrites).

	exclude(_, [], []).
exclude(Key, [Head|Tail], Result) :-
	( is_key(Key, Head) ->
		Tail = Result
	; Result = [Head|FilteredTail],
		exclude(Key, Tail, FilteredTail)
	).

fetch_from_host(Key, Value) :-
	setup_call_cleanup(
	socket_client_open(localhost:12345, Stream, []),
	(
		format(Stream, "sload ~w\n", [Key]),
		flush_output(Stream),
		read_term(Stream, Response, []),
		( Response = value(V) -> 
			Value = V
		; Response = error(Msg) ->
			format("Error from server: ~w~n", [Msg]),
			fail
		; write('Unexpected response format.'), nl, fail
		)
	),
	close(Stream)
).

store_on_host(Key, Value) :-
	format("Storing ~w ~w\n", [Key, Value]),
	setup_call_cleanup(
	socket_client_open(localhost:12345, Stream, []),
	(
		format(Stream, "sstore ~w ~w\n", [Key, Value]),
		flush_output(Stream),
		read_term(Stream, Response, []),
		( Response = ok -> 
			true
		; Response = error(Msg) ->
			format("Error from server: ~w~n", [Msg]),
			fail
		; write('Unexpected response format.'), nl, fail
		)
	),
	close(Stream)
).

commit_writes :-
	bb_get(pending_writes, PendingWrites),
	reverse(PendingWrites, PendingWritesReversed),
	forall(member(Key-Value, PendingWritesReversed), store_on_host(Key, Value)),
	bb_put(pending_writes, []).
