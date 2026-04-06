package io.logisticos.driver.core.database.dao

import android.content.Context
import androidx.room.Room
import androidx.test.core.app.ApplicationProvider
import io.logisticos.driver.core.database.DriverDatabase
import io.logisticos.driver.core.database.entity.TaskEntity
import io.logisticos.driver.core.database.entity.TaskStatus
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.Assertions.*

class TaskDaoTest {
    private lateinit var db: DriverDatabase
    private lateinit var dao: TaskDao

    @BeforeEach fun setUp() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        db = Room.inMemoryDatabaseBuilder(context, DriverDatabase::class.java)
            .allowMainThreadQueries()
            .build()
        dao = db.taskDao()
    }

    @AfterEach fun tearDown() { db.close() }

    @Test
    fun `insert and retrieve task by id`() = runTest {
        val task = TaskEntity(
            id = "task-1", shiftId = "shift-1", awb = "LS-ABC123",
            recipientName = "Juan dela Cruz", recipientPhone = "+63912345678",
            address = "123 Rizal St, Makati", lat = 14.55, lng = 121.03,
            status = TaskStatus.ASSIGNED, stopOrder = 1,
            requiresPhoto = true, requiresSignature = false, requiresOtp = false,
            isCod = false, codAmount = 0.0, syncedAt = null
        )
        dao.insert(task)
        val retrieved = dao.getById("task-1")
        assertEquals("Juan dela Cruz", retrieved?.recipientName)
    }

    @Test
    fun `getTasksForShift returns flow of tasks ordered by stopOrder`() = runTest {
        dao.insert(TaskEntity(id = "t2", shiftId = "s1", awb = "LS-2", recipientName = "B",
            recipientPhone = "", address = "", lat = 0.0, lng = 0.0,
            status = TaskStatus.ASSIGNED, stopOrder = 2,
            requiresPhoto = false, requiresSignature = false, requiresOtp = false,
            isCod = false, codAmount = 0.0, syncedAt = null))
        dao.insert(TaskEntity(id = "t1", shiftId = "s1", awb = "LS-1", recipientName = "A",
            recipientPhone = "", address = "", lat = 0.0, lng = 0.0,
            status = TaskStatus.ASSIGNED, stopOrder = 1,
            requiresPhoto = false, requiresSignature = false, requiresOtp = false,
            isCod = false, codAmount = 0.0, syncedAt = null))
        val tasks = dao.getTasksForShift("s1").first()
        assertEquals("t1", tasks[0].id)
        assertEquals("t2", tasks[1].id)
    }

    @Test
    fun `updateStatus changes task status`() = runTest {
        val task = TaskEntity(id = "task-1", shiftId = "shift-1", awb = "LS-1",
            recipientName = "A", recipientPhone = "", address = "", lat = 0.0, lng = 0.0,
            status = TaskStatus.ASSIGNED, stopOrder = 1,
            requiresPhoto = false, requiresSignature = false, requiresOtp = false,
            isCod = false, codAmount = 0.0, syncedAt = null)
        dao.insert(task)
        dao.updateStatus("task-1", TaskStatus.COMPLETED)
        assertEquals(TaskStatus.COMPLETED, dao.getById("task-1")?.status)
    }
}
