balance_of(User, Value, State) :-
	eth_sload(User, Value, State).

mint(User, Value, State, NewState) :-
	balance_of(User, OldValue, State),
	NewValue is OldValue + Value,
	eth_sstore(User, NewValue, State, NewState).

transfer(From, To, Value, State, NewState) :-
	balance_of(From, FromValue, State),
	balance_of(To, ToValue, State),
	FromValue >= Value,
	NewFromValue is FromValue - Value,
	NewToValue is ToValue + Value,
	eth_sstore(From, NewFromValue, State, State1),
	eth_sstore(To, NewToValue, State1, NewState).

erc20(Calldata, State, NewState, Result) :-
	Calldata =.. [Function | Args],
	dispatch(Function, Args, State, NewState, Result).

dispatch(balanceOf, [User], State, NewState, Result) :-
	balance_of(User, Result, State),
	NewState = State.

dispatch(mint, [User, Value], State, NewState, 1) :-
	mint(User, Value, State, NewState).

dispatch(transfer, [From, To, Value], State, NewState, 1) :-
	transfer(From, To, Value, State, NewState).

main(Calldata, NewState, Result) :-
	init_pending_writes,
	State = [],
	erc20(Calldata, State, NewState, Result),
	format("State: ~w~n", [NewState]),
	format("Result: ~w~n", [Result]),
	commit_writes,
	eth_return(Result).
