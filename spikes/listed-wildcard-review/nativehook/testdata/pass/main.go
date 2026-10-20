package main
import "fmt"
//hybrid:checked
func worker(x int,done chan int){done<-x+1}
func main(){done:=make(chan int);go worker(41,done);fmt.Println(<-done)}
