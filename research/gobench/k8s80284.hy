// kubernetes/80284: two closures capture the same *authenticator and write its
// field. Func-typed fields (`onRotate func()`) have no Hybrid spelling, so the
// field is modelled as an int — the share, not the field type, is the question.
struct authenticator {
    on_rotate int
}

fn (a *authenticator) update_transport_config() {
    a.on_rotate = 1
}

fn update_task(a *authenticator) {
    a.update_transport_config()
}

fn main() {
    a := authenticator{}
    task update_task(&a)
    task update_task(&a)
}