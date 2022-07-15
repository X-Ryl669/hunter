hunter() {
        env hunter
        test -e $XDG_RUNTIME_DIR/.hunter_cwd && source $XDG_RUNTIME_DIR/.hunter_cwd && rm $XDG_RUNTIME_DIR/.hunter_cwd 
        test -e ~/.hunter_cwd && source ~/.hunter_cwd && rm ~/.hunter_cwd
        test -d $F && cd $F || cd $HUNTER_CWD
}
