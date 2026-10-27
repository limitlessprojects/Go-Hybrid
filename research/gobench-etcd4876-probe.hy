const progress_interval = 10

struct server_watch_stream {}

fn (sws *server_watch_stream) send_loop() {
    _ = progress_interval
}

struct watch_server {}

fn (ws *watch_server) watch() {
    sws := server_watch_stream{}
    task sws.send_loop()
}

fn main() {
    ws := watch_server{}
    task ws.watch()
}